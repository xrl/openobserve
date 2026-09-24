// Copyright 2026 OpenObserve Inc.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

//! Global allocator statistics as `zo_allocator_bytes{allocator,stat}`.
//!
//! The gap between what the allocator holds and what Rust code has live is
//! retention (purge delay, arenas, per-thread caches). Without it, RSS growth
//! cannot be told apart from a growing cache.

use std::time::Duration;

#[cfg(any(feature = "mimalloc", feature = "jemalloc"))]
use config::metrics::ALLOCATOR_BYTES;

#[cfg(any(feature = "mimalloc", feature = "jemalloc"))]
const INTERVAL: Duration = Duration::from_secs(15);

pub fn start() {
    #[cfg(any(feature = "mimalloc", feature = "jemalloc"))]
    tokio::spawn(async {
        let mut interval = tokio::time::interval(INTERVAL);
        loop {
            interval.tick().await;
            for (allocator, stat, bytes) in read() {
                ALLOCATOR_BYTES
                    .with_label_values(&[allocator, stat])
                    .set(bytes as i64);
            }
        }
    });
}

#[cfg(feature = "mimalloc")]
fn read() -> Vec<(&'static str, &'static str, usize)> {
    let (mut elapsed, mut user, mut system) = (0usize, 0usize, 0usize);
    let (mut rss, mut peak_rss, mut commit, mut peak_commit, mut faults) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    // SAFETY: every argument is a valid, exclusive pointer to a local usize.
    unsafe {
        libmimalloc_sys::mi_process_info(
            &mut elapsed,
            &mut user,
            &mut system,
            &mut rss,
            &mut peak_rss,
            &mut commit,
            &mut peak_commit,
            &mut faults,
        );
    }
    vec![
        ("mimalloc", "committed", commit),
        ("mimalloc", "committed_peak", peak_commit),
    ]
}

#[cfg(all(feature = "jemalloc", not(feature = "mimalloc")))]
fn read() -> Vec<(&'static str, &'static str, usize)> {
    use tikv_jemalloc_ctl::{epoch, stats};
    if epoch::advance().is_err() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(6);
    for (stat, value) in [
        ("allocated", stats::allocated::read()),
        ("active", stats::active::read()),
        ("resident", stats::resident::read()),
        ("mapped", stats::mapped::read()),
        ("retained", stats::retained::read()),
        ("metadata", stats::metadata::read()),
    ] {
        if let Ok(v) = value {
            out.push(("jemalloc", stat, v));
        }
    }
    out
}
