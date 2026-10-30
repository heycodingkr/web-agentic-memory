use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const DEFAULT_MAX_IN_FLIGHT_REQUESTS: usize = 256;

/// Per-process backpressure for API and MCP requests that may touch shared services.
#[derive(Clone)]
pub struct LoadShedGate {
    slots: Arc<Semaphore>,
}

impl LoadShedGate {
    pub fn new(max_in_flight: usize) -> Option<Self> {
        if max_in_flight == 0 || max_in_flight > Semaphore::MAX_PERMITS {
            return None;
        }
        Some(Self {
            slots: Arc::new(Semaphore::new(max_in_flight)),
        })
    }

    pub fn from_env() -> Self {
        let max_in_flight = match std::env::var("WAM_MAX_INFLIGHT_REQUESTS") {
            Ok(value) => match value.parse::<usize>() {
                Ok(value) if value > 0 && value <= Semaphore::MAX_PERMITS => value,
                _ => {
                    eprintln!("invalid WAM_MAX_INFLIGHT_REQUESTS; using default");
                    DEFAULT_MAX_IN_FLIGHT_REQUESTS
                }
            },
            Err(std::env::VarError::NotPresent) => DEFAULT_MAX_IN_FLIGHT_REQUESTS,
            Err(std::env::VarError::NotUnicode(_)) => {
                eprintln!("invalid WAM_MAX_INFLIGHT_REQUESTS; using default");
                DEFAULT_MAX_IN_FLIGHT_REQUESTS
            }
        };
        Self::new(max_in_flight).expect("the default in-flight limit is valid")
    }

    pub fn try_acquire(&self) -> Option<OwnedSemaphorePermit> {
        Arc::clone(&self.slots).try_acquire_owned().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::LoadShedGate;

    #[test]
    fn in_flight_gate_rejects_excess_and_recovers_when_work_finishes() {
        let gate = LoadShedGate::new(1).unwrap();
        let permit = gate.try_acquire().unwrap();
        assert!(gate.try_acquire().is_none());

        drop(permit);

        assert!(gate.try_acquire().is_some());
    }

    #[test]
    fn zero_capacity_is_rejected_at_configuration_boundary() {
        assert!(LoadShedGate::new(0).is_none());
    }
}
