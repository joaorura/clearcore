#![forbid(unsafe_code)]

use realtime_noise_supervisor::EngineSupervisor;

#[derive(Default)]
pub struct ServiceConfig {
    pub socket_path: Option<String>,
}

pub struct ServiceBootstrap {
    pub supervisor: EngineSupervisor,
}

impl Default for ServiceBootstrap {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceBootstrap {
    #[must_use]
    pub fn new() -> Self {
        Self {
            supervisor: EngineSupervisor::default(),
        }
    }
}
