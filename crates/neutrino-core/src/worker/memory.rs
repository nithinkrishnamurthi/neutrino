use std::fs;
use std::io::{self, BufRead};
use tracing::warn;

/// Get the memory usage of a process in MB by reading /proc/<pid>/status
/// Returns RSS (Resident Set Size) in megabytes
pub fn get_process_memory_mb(pid: u32) -> Result<u64, io::Error> {
    let status_path = format!("/proc/{}/status", pid);
    let file = fs::File::open(&status_path)?;
    let reader = io::BufReader::new(file);

    // Parse /proc/<pid>/status to find VmRSS line
    for line in reader.lines() {
        let line = line?;
        if line.starts_with("VmRSS:") {
            // Line format: "VmRSS:     123456 kB"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(kb) = parts[1].parse::<u64>() {
                    // Convert from KB to MB
                    return Ok(kb / 1024);
                }
            }
            warn!("Failed to parse VmRSS value from: {}", line);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Failed to parse VmRSS",
            ));
        }
    }

    // VmRSS not found
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "VmRSS not found in /proc/<pid>/status",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_current_process_memory() {
        // Test with current process
        let pid = std::process::id();
        let result = get_process_memory_mb(pid);

        // Should succeed on Linux systems
        #[cfg(target_os = "linux")]
        {
            assert!(result.is_ok());
            let memory_mb = result.unwrap();
            // Process should use at least some memory
            assert!(memory_mb > 0);
            println!("Current process memory: {} MB", memory_mb);
        }
    }

    #[test]
    fn test_invalid_pid() {
        // Test with invalid PID
        let result = get_process_memory_mb(999999);
        assert!(result.is_err());
    }
}
