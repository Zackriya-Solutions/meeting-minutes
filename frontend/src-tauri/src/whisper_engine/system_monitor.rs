use sysinfo::System;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;
use log::{info, warn, debug};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemResources {
    pub memory_used_percent: f32,
    pub cpu_usage_percent: f32,
    pub cpu_temperature_celsius: Option<f32>,
    pub available_memory_mb: u64,
    pub total_memory_mb: u64,
    pub cpu_cores: usize,
}

#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub max_memory_percent: f32,      // Default: 70%
    pub max_cpu_percent: f32,         // Default: 80%
    pub max_cpu_temperature: f32,     // Default: 85°C
    pub worker_memory_budget_mb: u64, // Memory budget per worker
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_percent: 70.0,
            max_cpu_percent: 80.0,
            max_cpu_temperature: 85.0,
            worker_memory_budget_mb: 512, // 512MB per worker default
        }
    }
}

pub struct SystemMonitor {
    system: Arc<RwLock<System>>,
    limits: ResourceLimits,
    monitoring_enabled: bool,
}

impl SystemMonitor {
    pub fn new() -> Self {
        info!("Initializing system monitor");
        let mut system = System::new_all();
        system.refresh_all();

        Self {
            system: Arc::new(RwLock::new(system)),
            limits: ResourceLimits::default(),
            monitoring_enabled: true,
        }
    }

    pub fn with_limits(limits: ResourceLimits) -> Self {
        let mut monitor = Self::new();
        monitor.limits = limits;
        monitor
    }

    pub async fn refresh_system_info(&self) -> Result<()> {
        if !self.monitoring_enabled {
            return Ok(());
        }

        let mut system = self.system.write().await;
        system.refresh_all();

        // Wait a bit for accurate CPU readings (sysinfo requirement)
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        system.refresh_cpu_all();

        Ok(())
    }

    pub async fn get_current_resources(&self) -> Result<SystemResources> {
        let system = self.system.read().await;

        let total_memory = system.total_memory();
        let used_memory = system.used_memory();
        let memory_used_percent = (used_memory as f32 / total_memory as f32) * 100.0;

        // Get average CPU usage across all cores
        let cpu_usage_percent = system.cpus().iter()
            .map(|cpu| cpu.cpu_usage())
            .sum::<f32>() / system.cpus().len() as f32;

        // Try to get CPU temperature from components
        let cpu_temperature_celsius = self.get_cpu_temperature(&system).await;

        let resources = SystemResources {
            memory_used_percent,
            cpu_usage_percent,
            cpu_temperature_celsius,
            available_memory_mb: (total_memory - used_memory) / 1024 / 1024,
            total_memory_mb: total_memory / 1024 / 1024,
            cpu_cores: system.cpus().len(),
        };

        debug!("Current resources: Memory: {:.1}%, CPU: {:.1}%, Temp: {:?}°C",
               resources.memory_used_percent,
               resources.cpu_usage_percent,
               resources.cpu_temperature_celsius);

        Ok(resources)
    }

    async fn get_cpu_temperature(&self, _system: &System) -> Option<f32> {
        // Temperature monitoring is optional and varies by platform
        // For now, we'll disable temperature monitoring to avoid API compatibility issues
        // This can be re-enabled once the sysinfo API is stable
        // TODO: Implement platform-specific temperature reading if needed
        None
    }

    pub async fn check_resource_constraints(&self) -> Result<ResourceStatus> {
        let resources = self.get_current_resources().await?;

        let mut status = ResourceStatus {
            can_proceed: true,
            memory_ok: true,
            cpu_ok: true,
            temperature_ok: true,
            warnings: Vec::new(),
        };

        // Check memory constraints
        if resources.memory_used_percent > self.limits.max_memory_percent {
            status.can_proceed = false;
            status.memory_ok = false;
            status.warnings.push(format!(
                "Memory usage too high: {:.1}% > {:.1}%",
                resources.memory_used_percent,
                self.limits.max_memory_percent
            ));
            warn!("Memory constraint violated: {:.1}%", resources.memory_used_percent);
        }

        // Check CPU constraints
        if resources.cpu_usage_percent > self.limits.max_cpu_percent {
            status.can_proceed = false;
            status.cpu_ok = false;
            status.warnings.push(format!(
                "CPU usage too high: {:.1}% > {:.1}%",
                resources.cpu_usage_percent,
                self.limits.max_cpu_percent
            ));
            warn!("CPU constraint violated: {:.1}%", resources.cpu_usage_percent);
        }

        // Check temperature constraints
        if let Some(temp) = resources.cpu_temperature_celsius {
            if temp > self.limits.max_cpu_temperature {
                status.can_proceed = false;
                status.temperature_ok = false;
                status.warnings.push(format!(
                    "CPU temperature too high: {:.1}°C > {:.1}°C",
                    temp,
                    self.limits.max_cpu_temperature
                ));
                warn!("Temperature constraint violated: {:.1}°C", temp);
            }
        }

        Ok(status)
    }

    pub async fn calculate_safe_worker_count(&self) -> Result<usize> {
        let resources = self.get_current_resources().await?;

        // Calculate based on available memory
        let available_memory_mb = resources.available_memory_mb as f32 * (self.limits.max_memory_percent / 100.0);
        let memory_based_workers = (available_memory_mb / self.limits.worker_memory_budget_mb as f32) as usize;

        // Calculate based on CPU cores (never exceed CPU count)
        let cpu_based_workers = resources.cpu_cores;

        // Take the minimum and cap at 4 workers max (as per safety plan)
        let safe_workers = std::cmp::min(
            std::cmp::min(memory_based_workers, cpu_based_workers),
            4
        ).max(1); // Always allow at least 1 worker

        info!("Calculated safe worker count: {} (memory: {}, cpu: {}, capped at 4)",
              safe_workers, memory_based_workers, cpu_based_workers);

        Ok(safe_workers)
    }

    pub fn set_monitoring_enabled(&mut self, enabled: bool) {
        self.monitoring_enabled = enabled;
        if enabled {
            info!("System monitoring enabled");
        } else {
            info!("System monitoring disabled");
        }
    }

    pub fn get_limits(&self) -> &ResourceLimits {
        &self.limits
    }

    pub fn update_limits(&mut self, limits: ResourceLimits) {
        info!("Updating resource limits: memory: {:.1}%, cpu: {:.1}%, temp: {:.1}°C",
               limits.max_memory_percent, limits.max_cpu_percent, limits.max_cpu_temperature);
        self.limits = limits;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStatus {
    pub can_proceed: bool,
    pub memory_ok: bool,
    pub cpu_ok: bool,
    pub temperature_ok: bool,
    pub warnings: Vec<String>,
}

impl ResourceStatus {
    pub fn is_healthy(&self) -> bool {
        self.memory_ok && self.cpu_ok && self.temperature_ok
    }

    pub fn get_primary_constraint(&self) -> Option<String> {
        if !self.memory_ok {
            Some("Memory usage too high".to_string())
        } else if !self.temperature_ok {
            Some("CPU temperature too high".to_string())
        } else if !self.cpu_ok {
            Some("CPU usage too high".to_string())
        } else {
            None
        }
    }
}

// Helper function to create a system monitor with common settings
pub fn create_system_monitor() -> SystemMonitor {
    SystemMonitor::new()
}

// Helper function to create a system monitor with custom limits
pub fn create_system_monitor_with_limits(
    max_memory_percent: f32,
    max_cpu_percent: f32,
    max_cpu_temperature: f32,
) -> SystemMonitor {
    let limits = ResourceLimits {
        max_memory_percent,
        max_cpu_percent,
        max_cpu_temperature,
        worker_memory_budget_mb: 512,
    };
    SystemMonitor::with_limits(limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_system_monitor_creation() {
        let monitor = SystemMonitor::new();
        assert!(monitor.monitoring_enabled);
    }

    #[tokio::test]
    async fn test_get_current_resources() {
        let monitor = SystemMonitor::new();
        let resources = monitor.get_current_resources().await.unwrap();

        assert!(resources.memory_used_percent >= 0.0);
        assert!(resources.memory_used_percent <= 100.0);
        assert!(resources.cpu_usage_percent >= 0.0);
        assert!(resources.total_memory_mb > 0);
        assert!(resources.cpu_cores > 0);
    }

    #[tokio::test]
    async fn test_safe_worker_count() {
        let monitor = SystemMonitor::new();
        let worker_count = monitor.calculate_safe_worker_count().await.unwrap();

        assert!(worker_count >= 1);
        assert!(worker_count <= 4);
    }
}