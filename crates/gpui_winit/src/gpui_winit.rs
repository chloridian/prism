//! A GPUI platform built on winit.
//!
//! Ported from WGPUI (`Far-Beyond-Pulsar/WGPUI`) at commit
//! `f0aa74604df9e2f6a319a7628ed805f40f387355`.

#![cfg(target_os = "windows")]

mod dispatcher;
mod display;
mod input;
