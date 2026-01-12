// MIT License
//
// Copyright (c) 2021 Hajime Nakagami<nakagami@gmail.com>
// Copyright (c) 2026 Roberto (eventos implementation)
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Event Alerter for Firebird POST_EVENT notifications
//!
//! This module provides functionality to listen for database events
//! posted via POST_EVENT in Firebird stored procedures or triggers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use super::error::Error;
use super::Connection;

/// Maximum number of events per alerter (Firebird limit)
pub const MAX_EVENTS: usize = 15;

/// Event Parameter Block version
const EPB_VERSION1: u8 = 1;

/// Event notification callback type
pub type EventCallback = Box<dyn Fn(&str, u32) + Send + 'static>;

/// Event Alerter for listening to Firebird POST_EVENT notifications
///
/// # Example
/// ```ignore
/// let mut alerter = EventAlerter::new("firebird://SYSDBA:pass@localhost/test.fdb")?;
/// alerter.register(&["my_event", "another_event"])?;
/// alerter.start(|event_name, count| {
///     println!("Event '{}' fired {} times", event_name, count);
/// })?;
/// // ... later
/// alerter.stop()?;
/// ```
pub struct EventAlerter {
    conn_string: String,
    events: Vec<String>,
    event_buffer: Vec<u8>,
    result_buffer: Vec<u8>,
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl EventAlerter {
    /// Create a new EventAlerter with the given connection string
    pub fn new(conn_string: &str) -> Self {
        Self {
            conn_string: conn_string.to_string(),
            events: Vec::new(),
            event_buffer: Vec::new(),
            result_buffer: Vec::new(),
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    /// Register events to listen for
    ///
    /// # Arguments
    /// * `events` - Slice of event names (max 15)
    ///
    /// # Errors
    /// Returns error if more than 15 events are specified
    pub fn register(&mut self, events: &[&str]) -> Result<(), Error> {
        if events.len() > MAX_EVENTS {
            return Err(Error::PoolError(format!(
                "Maximum {} events allowed, got {}",
                MAX_EVENTS,
                events.len()
            )));
        }

        self.events = events.iter().map(|s| s.to_string()).collect();
        self.event_buffer = build_event_buffer(&self.events);
        self.result_buffer = self.event_buffer.clone();

        Ok(())
    }

    /// Start listening for events
    ///
    /// This spawns a background thread that maintains a separate connection
    /// to receive event notifications.
    ///
    /// # Arguments
    /// * `callback` - Function to call when an event is received
    pub fn start<F>(&mut self, callback: F) -> Result<(), Error>
    where
        F: Fn(&str, u32) + Send + 'static,
    {
        if self.events.is_empty() {
            return Err(Error::PoolError(
                "No events registered. Call register() first.".to_string(),
            ));
        }

        if self.running.load(Ordering::SeqCst) {
            return Err(Error::PoolError("Alerter already running".to_string()));
        }

        self.running.store(true, Ordering::SeqCst);

        let conn_string = self.conn_string.clone();
        let events = self.events.clone();
        let event_buffer = self.event_buffer.clone();
        let running = self.running.clone();
        let callback = Box::new(callback);

        let handle = thread::spawn(move || {
            if let Err(e) = event_loop(&conn_string, &events, &event_buffer, running.clone(), callback) {
                eprintln!("Event alerter error: {:?}", e);
            }
            running.store(false, Ordering::SeqCst);
        });

        self.handle = Some(handle);
        Ok(())
    }

    /// Stop listening for events
    pub fn stop(&mut self) -> Result<(), Error> {
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.handle.take() {
            // Wait for thread to finish (with timeout would be better)
            let _ = handle.join();
        }

        Ok(())
    }

    /// Check if the alerter is currently running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get the list of registered events
    pub fn events(&self) -> &[String] {
        &self.events
    }
}

impl Drop for EventAlerter {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Build an Event Parameter Block from event names
///
/// Format:
/// - EPB_version1 (1 byte)
/// - For each event:
///   - name_length (1 byte)
///   - name (name_length bytes)
///   - count (4 bytes, little-endian, initially 0)
fn build_event_buffer(events: &[String]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(EPB_VERSION1);

    for event in events {
        let name = event.trim();
        buf.push(name.len() as u8);
        buf.extend_from_slice(name.as_bytes());
        // Initial count = 0 (4 bytes little-endian)
        buf.extend_from_slice(&[0u8; 4]);
    }

    buf
}

/// Parse event counts from result buffer and compare with previous
fn parse_event_counts(events: &[String], event_buffer: &[u8], result_buffer: &[u8]) -> Vec<(String, u32)> {
    let mut fired = Vec::new();

    if event_buffer.len() != result_buffer.len() || event_buffer.is_empty() {
        return fired;
    }

    let mut pos = 1; // Skip EPB_version1

    for event in events {
        if pos >= event_buffer.len() {
            break;
        }

        let name_len = event_buffer[pos] as usize;
        pos += 1 + name_len; // Skip name_len + name

        if pos + 4 > event_buffer.len() {
            break;
        }

        let old_count = u32::from_le_bytes([
            event_buffer[pos],
            event_buffer[pos + 1],
            event_buffer[pos + 2],
            event_buffer[pos + 3],
        ]);

        let new_count = u32::from_le_bytes([
            result_buffer[pos],
            result_buffer[pos + 1],
            result_buffer[pos + 2],
            result_buffer[pos + 3],
        ]);

        if new_count > old_count {
            fired.push((event.clone(), new_count - old_count));
        }

        pos += 4;
    }

    fired
}

/// Main event loop - runs in a separate thread
fn event_loop(
    conn_string: &str,
    events: &[String],
    initial_buffer: &[u8],
    running: Arc<AtomicBool>,
    callback: EventCallback,
) -> Result<(), Error> {
    // Open a dedicated connection for events
    let conn = Connection::connect(conn_string)?;

    let mut event_buffer = initial_buffer.to_vec();

    // Register for events
    let mut event_id = conn.queue_events(&event_buffer)?;

    while running.load(Ordering::SeqCst) {
        // Wait for event (with timeout to check running flag)
        match conn.wait_for_event(event_id, 1000) {
            Ok(Some(result_buffer)) => {
                // Parse which events fired
                let fired = parse_event_counts(events, &event_buffer, &result_buffer);

                // Call callback for each fired event
                for (name, count) in fired {
                    callback(&name, count);
                }

                // Update event buffer with new counts
                event_buffer = result_buffer;

                // Re-queue for more events
                event_id = conn.queue_events(&event_buffer)?;
            }
            Ok(None) => {
                // Timeout, just continue
            }
            Err(e) => {
                if running.load(Ordering::SeqCst) {
                    return Err(e);
                }
                break;
            }
        }
    }

    // Cancel events on exit
    let _ = conn.cancel_events(event_id);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_event_buffer() {
        let events = vec!["test".to_string(), "foo".to_string()];
        let buf = build_event_buffer(&events);

        // EPB_version1 + (1 + 4 + 4) + (1 + 3 + 4) = 1 + 9 + 8 = 18
        assert_eq!(buf.len(), 18);
        assert_eq!(buf[0], EPB_VERSION1);
        assert_eq!(buf[1], 4); // "test" length
        assert_eq!(&buf[2..6], b"test");
        assert_eq!(&buf[6..10], &[0, 0, 0, 0]); // count
        assert_eq!(buf[10], 3); // "foo" length
        assert_eq!(&buf[11..14], b"foo");
        assert_eq!(&buf[14..18], &[0, 0, 0, 0]); // count
    }

    #[test]
    fn test_parse_event_counts() {
        let events = vec!["test".to_string()];

        // Old buffer: count = 5
        let mut old_buf = vec![EPB_VERSION1, 4];
        old_buf.extend_from_slice(b"test");
        old_buf.extend_from_slice(&5u32.to_le_bytes());

        // New buffer: count = 8
        let mut new_buf = vec![EPB_VERSION1, 4];
        new_buf.extend_from_slice(b"test");
        new_buf.extend_from_slice(&8u32.to_le_bytes());

        let fired = parse_event_counts(&events, &old_buf, &new_buf);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].0, "test");
        assert_eq!(fired[0].1, 3); // 8 - 5 = 3 new events
    }

    #[test]
    fn test_register_too_many_events() {
        let mut alerter = EventAlerter::new("firebird://test");
        let events: Vec<&str> = (0..20).map(|i| "event").collect();

        let result = alerter.register(&events);
        assert!(result.is_err());
    }
}
