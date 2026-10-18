/* Copyright 2026 Mission Center Developers
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! One resident launcher per session: it owns `LAUNCHER_ID` on the session bus, and every later
//! launch (the Meta key) asks it to show or hide itself, then exits.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use zbus::blocking::{Connection, connection::Builder};

pub const LAUNCHER_ID: &str = "io.github.taskmanager.TaskManager.Launcher";
const PATH: &str = "/Launcher";

struct Toggle {
    requested: Arc<AtomicBool>,
    ctx: Arc<OnceLock<egui::Context>>,
}

#[zbus::interface(name = "io.github.taskmanager.TaskManager.Launcher")]
impl Toggle {
    fn toggle(&self) {
        self.requested.store(true, Ordering::Release);
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint();
        }
    }
}

pub struct Instance {
    _connection: Connection,
    requested: Arc<AtomicBool>,
    ctx: Arc<OnceLock<egui::Context>>,
}

#[derive(Debug)]
pub enum ClaimError {
    BusUnavailable(zbus::Error),
    Failed(zbus::Error),
}

impl Instance {
    /// `Ok(None)`: a running launcher was toggled instead.
    pub fn claim() -> Result<Option<Self>, ClaimError> {
        let requested = Arc::new(AtomicBool::new(false));
        let ctx = Arc::new(OnceLock::new());
        let connection = Builder::session()
            .map_err(ClaimError::BusUnavailable)?
            .method_timeout(Duration::from_secs(2))
            .serve_at(
                PATH,
                Toggle {
                    requested: requested.clone(),
                    ctx: ctx.clone(),
                },
            )
            .map_err(ClaimError::Failed)?
            .build()
            .map_err(ClaimError::BusUnavailable)?;
        let request_name = || {
            connection.request_name_with_flags(
                LAUNCHER_ID,
                zbus::fdo::RequestNameFlags::DoNotQueue.into(),
            )
        };
        match request_name() {
            Ok(_) => {}
            Err(zbus::Error::NameTaken) => {
                match connection.call_method(
                    Some(LAUNCHER_ID),
                    PATH,
                    Some(LAUNCHER_ID),
                    "Toggle",
                    &(),
                ) {
                    Ok(_) => return Ok(None),
                    // The owner went away meanwhile: take its place, never queue behind a racer
                    Err(error) => request_name().map_err(|_| ClaimError::Failed(error))?,
                };
            }
            Err(error) => return Err(ClaimError::Failed(error)),
        }
        Ok(Some(Self {
            _connection: connection,
            requested,
            ctx,
        }))
    }
}

impl crate::layer::Activation for Instance {
    fn take(&self) -> bool {
        self.requested.swap(false, Ordering::AcqRel)
    }

    fn attach(&mut self, ctx: egui::Context) {
        let _ = self.ctx.set(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::Activation;

    #[test]
    #[ignore = "requires an isolated session bus; run with dbus-run-session and --test-threads=1"]
    fn second_launch_toggles_the_running_launcher() {
        let owner = Instance::claim().unwrap().unwrap();
        assert!(!owner.take());
        assert!(Instance::claim().unwrap().is_none());
        assert!(owner.take());
        assert!(!owner.take());
    }
}
