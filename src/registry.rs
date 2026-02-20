// src/registry.rs
//! Phase 6: Name Registry
//! Allows actors to be referenced by string names instead of raw PIDs.

use crate::pid::Pid;
use dashmap::DashMap;

pub struct NameRegistry {
    /// Mapping of human-readable names to PIDs.
    names: DashMap<String, Pid>,
}

impl NameRegistry {
    /// Create a new, empty name registry.
    pub fn new() -> Self {
        Self {
            names: DashMap::new(),
        }
    }

    /// Register a PID under a specific name.
    /// If the name already exists, it will be overwritten.
    pub fn register(&self, name: String, pid: Pid) {
        self.names.insert(name, pid);
    }

    /// Retrieve the PID associated with a name.
    pub fn resolve(&self, name: &str) -> Option<Pid> {
        self.names.get(name).map(|p| *p)
    }

    /// Remove a name mapping.
    pub fn unregister(&self, name: &str) {
        self.names.remove(name);
    }

    /// List registered entries whose path begins with `prefix`.
    /// If `prefix` is empty or "/" this returns all entries.
    pub fn list_children(&self, prefix: &str) -> Vec<(String, Pid)> {
        let mut out = Vec::new();
        let norm = if prefix.is_empty() || prefix == "/" {
            String::new()
        } else if prefix.ends_with('/') {
            prefix.trim_end_matches('/').to_string()
        } else {
            prefix.to_string()
        };

        let matcher = if norm.is_empty() { None } else { Some(format!("{}/", norm)) };

        for r in self.names.iter() {
            let key = r.key();
            if let Some(ref m) = matcher {
                if key.starts_with(m) {
                    out.push((key.clone(), *r.value()));
                }
            } else {
                out.push((key.clone(), *r.value()));
            }
        }

        out
    }

    /// List only direct children one level below `prefix`.
    /// For prefix `/a/b` entries returned will be `/a/b/child` but not `/a/b/child/grand`.
    pub fn list_direct_children(&self, prefix: &str) -> Vec<(String, Pid)> {
        let mut out = Vec::new();
        let norm = if prefix.is_empty() || prefix == "/" {
            String::new()
        } else if prefix.ends_with('/') {
            prefix.trim_end_matches('/').to_string()
        } else {
            prefix.to_string()
        };

        let matcher = if norm.is_empty() { 
            String::new()
        } else { 
            format!("{}/", norm)
        };

        for r in self.names.iter() {
            let key = r.key();
            if !matcher.is_empty() {
                if !key.starts_with(&matcher) {
                    continue;
                }
                let tail = &key[matcher.len()..];
                if tail.contains('/') {
                    continue; // deeper than one level
                }
                out.push((key.clone(), *r.value()));
            } else {
                // root: direct children are top-level entries without additional '/'
                let tail = if key.starts_with('/') { &key[1..] } else { &key[..] };
                if tail.contains('/') {
                    continue;
                }
                out.push((key.clone(), *r.value()));
            }
        }

        out
    }
}
