//! A GPUI platform built on winit.
//!
//! Ported from WGPUI (`Far-Beyond-Pulsar/WGPUI`) at commit
//! `f0aa74604df9e2f6a319a7628ed805f40f387355`.

#![cfg(target_os = "windows")]

mod accessibility;
mod app;
mod capture;
mod clipboard;
mod credentials;
mod dispatcher;
mod display;
mod frame;
mod input;
mod keyboard;
mod platform;
mod renderer;
mod system;
mod text;
mod window;

pub use platform::WinitPlatform;
