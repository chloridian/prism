//! Task dispatch: background work on the Windows thread pool, main-thread work on a priority queue drained by the winit loop.

use std::{
    ffi::c_void,
    ptr::NonNull,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, ThreadId},
    time::Duration,
};

use anyhow::Context as _;
use gpui::{
    PlatformDispatcher, Priority, PriorityQueueReceiver, PriorityQueueSender, RunnableVariant,
    TimerResolutionGuard,
};
use gpui_util::ResultExt as _;
use windows::Win32::{
    Foundation::FILETIME,
    Media::{timeBeginPeriod, timeEndPeriod},
    System::Threading::{
        CloseThreadpoolTimer, CreateThreadpoolTimer, GetCurrentThread, PTP_CALLBACK_INSTANCE,
        PTP_TIMER, SetThreadPriority, SetThreadpoolTimer, THREAD_PRIORITY_TIME_CRITICAL,
        TP_CALLBACK_ENVIRON_V3, TP_CALLBACK_PRIORITY, TP_CALLBACK_PRIORITY_HIGH,
        TP_CALLBACK_PRIORITY_LOW, TP_CALLBACK_PRIORITY_NORMAL, TrySubmitThreadpoolCallback,
    },
};
use winit::event_loop::EventLoopProxy;

/// How long the main thread may run queued tasks before it yields to window events.
pub(crate) const MAIN_TASK_BUDGET: Duration = Duration::from_millis(10);

/// The event posted to the winit loop to drain the main-thread queue.
#[derive(Debug)]
pub(crate) struct Wake;

pub(crate) struct WinitDispatcher {
    main_sender: PriorityQueueSender<RunnableVariant>,
    main_thread_id: ThreadId,
    proxy: OnceLock<EventLoopProxy<Wake>>,
    wake_posted: AtomicBool,
}

impl WinitDispatcher {
    pub(crate) fn new() -> (Self, PriorityQueueReceiver<RunnableVariant>) {
        let (main_sender, main_receiver) = PriorityQueueReceiver::new();
        let dispatcher = Self {
            main_sender,
            main_thread_id: thread::current().id(),
            proxy: OnceLock::new(),
            wake_posted: AtomicBool::new(false),
        };
        (dispatcher, main_receiver)
    }

    /// Connects the dispatcher to the event loop. Tasks queued before this call run on the first wake.
    pub(crate) fn attach(&self, proxy: EventLoopProxy<Wake>) {
        if self.proxy.set(proxy).is_ok() {
            self.wake_posted.store(false, Ordering::Release);
            self.post_wake();
        }
    }

    /// Allows the next main-thread dispatch to wake the loop again.
    pub(crate) fn clear_wake(&self) {
        self.wake_posted.store(false, Ordering::Release);
    }

    /// Wakes the loop again without waiting for new work, so leftover tasks run after pending window events.
    pub(crate) fn rewake(&self) {
        self.wake_posted.store(true, Ordering::Release);
        self.post_wake();
    }

    pub(crate) fn mark_woken(&self) {
        self.wake_posted.store(true, Ordering::Release);
    }

    fn post_wake(&self) {
        if let Some(proxy) = self.proxy.get()
            && proxy.send_event(Wake).is_err()
        {
            log::debug!("event loop closed before the main thread was woken");
        }
    }

    fn dispatch_on_threadpool(&self, priority: TP_CALLBACK_PRIORITY, runnable: RunnableVariant) {
        let environ = TP_CALLBACK_ENVIRON_V3 {
            Version: 3,
            CallbackPriority: priority,
            Size: size_of::<TP_CALLBACK_ENVIRON_V3>() as u32,
            ..Default::default()
        };

        // A callback the pool never runs leaks the runnable. Dropping it instead would cancel its
        // task and panic the next awaiter, and the pool only skips callbacks during shutdown.
        let context = runnable.into_raw().as_ptr() as *mut c_void;
        unsafe {
            TrySubmitThreadpoolCallback(Some(run_work_callback), Some(context), Some(&environ))
                .log_err();
        }
    }

    fn dispatch_on_threadpool_after(&self, runnable: RunnableVariant, duration: Duration) {
        let context = runnable.into_raw().as_ptr() as *mut c_void;
        unsafe {
            if let Ok(timer) = CreateThreadpoolTimer(Some(run_timer_callback), Some(context), None)
            {
                let ticks = (duration.as_nanos() / 100).min(i64::MAX as u128) as i64;
                let due = (-ticks) as u64;
                let due_time = FILETIME {
                    dwLowDateTime: due as u32,
                    dwHighDateTime: (due >> 32) as u32,
                };
                SetThreadpoolTimer(timer, Some(&due_time), 0, None);
            }
        }
    }
}

pub(crate) fn execute_runnable(runnable: RunnableVariant) {
    let location = runnable.metadata().location;
    let spawned = runnable.metadata().spawned;
    gpui::profiler::update_running_task(spawned, location);
    runnable.run();
    gpui::profiler::save_task_timing();
}

impl PlatformDispatcher for WinitDispatcher {
    fn is_main_thread(&self) -> bool {
        thread::current().id() == self.main_thread_id
    }

    fn dispatch(&self, runnable: RunnableVariant, priority: Priority) {
        let priority = match priority {
            Priority::RealtimeAudio => {
                panic!("RealtimeAudio priority should use spawn_realtime, not dispatch")
            }
            Priority::High => TP_CALLBACK_PRIORITY_HIGH,
            Priority::Medium => TP_CALLBACK_PRIORITY_NORMAL,
            Priority::Low => TP_CALLBACK_PRIORITY_LOW,
        };
        self.dispatch_on_threadpool(priority, runnable);
    }

    fn dispatch_on_main_thread(&self, runnable: RunnableVariant, priority: Priority) {
        match self.main_sender.send(priority, runnable) {
            Ok(()) => {
                if !self.wake_posted.swap(true, Ordering::AcqRel) {
                    self.post_wake();
                }
            }
            Err(runnable) => {
                // The receiver is gone, so the app is shutting down. The runnable may wrap a !Send
                // future and this may not be the main thread, so it must not be dropped here.
                std::mem::forget(runnable);
            }
        }
    }

    fn dispatch_after(&self, duration: Duration, runnable: RunnableVariant) {
        self.dispatch_on_threadpool_after(runnable, duration);
    }

    fn spawn_realtime(&self, f: Box<dyn FnOnce() + Send>) {
        thread::spawn(move || {
            unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) }
                .context("thread priority")
                .log_err();
            f();
        });
    }

    fn increase_timer_resolution(&self) -> TimerResolutionGuard {
        unsafe {
            timeBeginPeriod(1);
        }
        gpui_util::defer(Box::new(|| unsafe {
            timeEndPeriod(1);
        }))
    }
}

unsafe extern "system" fn run_work_callback(
    _instance: PTP_CALLBACK_INSTANCE,
    context: *mut c_void,
) {
    let runnable = unsafe { RunnableVariant::from_raw(NonNull::new_unchecked(context as *mut ())) };
    execute_runnable(runnable);
}

unsafe extern "system" fn run_timer_callback(
    _instance: PTP_CALLBACK_INSTANCE,
    context: *mut c_void,
    timer: PTP_TIMER,
) {
    let runnable = unsafe { RunnableVariant::from_raw(NonNull::new_unchecked(context as *mut ())) };
    execute_runnable(runnable);
    unsafe { CloseThreadpoolTimer(timer) };
}
