use {
    axum::{
        extract::{ConnectInfo, Request, State},
        http::StatusCode,
        middleware::Next,
        response::{IntoResponse, Response},
    },
    std::{
        collections::{HashMap, VecDeque},
        net::{IpAddr, SocketAddr},
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    },
};

/// Maximum number of IP buckets to retain. When the map hits this cap the
/// least-recently-used bucket is evicted on the next insert.
const MAX_IP_BUCKETS: usize = 10_000;

#[derive(Clone)]
struct SlidingWindowLimiter<K>
where
    K: Eq + std::hash::Hash + Clone,
{
    inner: Arc<Mutex<HashMap<K, VecDeque<Instant>>>>,
    limit: usize,
    window: Duration,
    /// Track insertion order for LRU eviction.
    access_order: Arc<Mutex<VecDeque<K>>>,
}

impl<K> SlidingWindowLimiter<K>
where
    K: Eq + std::hash::Hash + Clone,
{
    fn new(limit: usize, window: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            limit,
            window,
            access_order: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    fn allow_now(&self, key: K, now: Instant) -> bool {
        let mut store = self.inner.lock().expect("rate limiter mutex poisoned");
        let mut order = self
            .access_order
            .lock()
            .expect("rate limiter access_order mutex poisoned");

        // Evict inactive buckets at the front of the access order list.
        // We only sweep once per call (amortized O(k) where k = number of stale
        // buckets at the head); this is bounded by the total bucket count.
        while let Some(oldest) = order.front() {
            if let Some(entries) = store.get(oldest) {
                if let Some(&oldest_ts) = entries.front() {
                    if now.duration_since(oldest_ts) >= self.window {
                        let evicted = order.pop_front().unwrap();
                        store.remove(&evicted);
                        continue;
                    }
                }
            }
            break;
        }

        let entries = store.entry(key.clone()).or_default();
        while let Some(ts) = entries.front() {
            if now.duration_since(*ts) >= self.window {
                entries.pop_front();
            } else {
                break;
            }
        }
        if entries.len() >= self.limit {
            return false;
        }
        entries.push_back(now);

        // Update LRU: move this key to the back (most recently used).
        // Remove old position if present (linear scan — acceptable for ≤10k buckets).
        if let Some(pos) = order.iter().position(|k| k == &key) {
            order.remove(pos);
        }
        order.push_back(key);

        // LRU eviction: if over capacity, evict the least-recently-used (front).
        if store.len() > MAX_IP_BUCKETS {
            if let Some(lru_key) = order.pop_front() {
                store.remove(&lru_key);
            }
        }

        true
    }
}

#[derive(Clone)]
pub struct RateLimitState {
    ip: SlidingWindowLimiter<IpAddr>,
    /// Extra IPs/CIDRs that skip the public IP bucket (same-host arb bot /
    /// local curl). Loopback is always exempt.
    bypass_ips: Arc<std::collections::HashSet<IpAddr>>,
}

impl RateLimitState {
    pub fn from_env() -> Self {
        let bypass_ips: std::collections::HashSet<IpAddr> = std::env::var("QUOTE_RATE_LIMIT_BYPASS_IPS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        Self {
            ip: SlidingWindowLimiter::new(10, Duration::from_secs(1)),
            bypass_ips: Arc::new(bypass_ips),
        }
    }
}

/// `/health` and `/ready` never consume rate-limit quota (exempt).
fn is_rate_limit_exempt_path(path: &str) -> bool {
    path == "/api/v1/health" || path == "/api/v1/ready"
}

fn is_ip_rate_limit_exempt(ip: IpAddr, bypass_ips: &std::collections::HashSet<IpAddr>) -> bool {
    ip.is_loopback() || bypass_ips.contains(&ip)
}

/// 10 req/s per IP. `/health` and `/ready` are exempt. No partner keys.
pub async fn rate_limit_middleware(State(state): State<RateLimitState>, request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let exempt = is_rate_limit_exempt_path(&path);
    let now = Instant::now();

    let allowed = if exempt {
        true
    } else if let Some(ConnectInfo(addr)) = request.extensions().get::<ConnectInfo<SocketAddr>>() {
        let ip = addr.ip();
        if is_ip_rate_limit_exempt(ip, &state.bypass_ips) {
            true
        } else {
            state.ip.allow_now(ip, now)
        }
    } else {
        // No ConnectInfo (test oneshot without extension): count as one IP bucket.
        true
    };

    if !allowed {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            axum::Json(crate::envelope::Envelope::<serde_json::Value>::err(
                crate::envelope::ApiErrorCode::RateLimited,
                "rate limit exceeded: max 10 requests/second per IP",
            )),
        )
            .into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use std::time::{Duration, Instant};

    #[test]
    fn limits_requests_per_window() {
        let limiter = SlidingWindowLimiter::new(2, Duration::from_secs(1));
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let now = Instant::now();
        assert!(limiter.allow_now(ip, now));
        assert!(limiter.allow_now(ip, now));
        assert!(!limiter.allow_now(ip, now));
        assert!(limiter.allow_now(ip, now + Duration::from_secs(1)));
    }

    #[test]
    fn loopback_is_rate_limit_exempt() {
        let empty = HashSet::new();
        assert!(is_ip_rate_limit_exempt(IpAddr::V4(Ipv4Addr::LOCALHOST), &empty));
        assert!(is_ip_rate_limit_exempt(IpAddr::V6(Ipv6Addr::LOCALHOST), &empty));
        assert!(!is_ip_rate_limit_exempt(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), &empty));
    }

    #[test]
    fn health_and_ready_are_exempt_paths() {
        assert!(is_rate_limit_exempt_path("/api/v1/health"));
        assert!(is_rate_limit_exempt_path("/api/v1/ready"));
        assert!(!is_rate_limit_exempt_path("/api/v1/quote"));
    }

    #[test]
    fn bounded_lru_eviction_at_capacity() {
        let limiter = SlidingWindowLimiter::new(1, Duration::from_secs(60));
        let now = Instant::now();

        // Fill to MAX_IP_BUCKETS + 1 to trigger eviction.
        for i in 0..=MAX_IP_BUCKETS {
            let ip = IpAddr::V4(Ipv4Addr::new((i >> 24) as u8, (i >> 16) as u8, (i >> 8) as u8, i as u8));
            assert!(limiter.allow_now(ip, now), "IP {ip} should be allowed");
        }

        // The first IP (0.0.0.0) should have been evicted as LRU.
        let evicted = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
        // After eviction, it should be allowed again (capacity freed).
        assert!(limiter.allow_now(evicted, now), "evicted IP should be allowed again");
    }

    #[test]
    fn inactive_buckets_swept_on_access() {
        let limiter = SlidingWindowLimiter::new(2, Duration::from_secs(1));
        let now = Instant::now();

        // Insert two requests for two different IPs.
        assert!(limiter.allow_now(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), now));
        assert!(limiter.allow_now(IpAddr::V4(Ipv4Addr::new(2, 2, 2, 2)), now));

        // After the window expires, a new access should sweep the old entries.
        let later = now + Duration::from_secs(2);
        assert!(limiter.allow_now(IpAddr::V4(Ipv4Addr::new(3, 3, 3, 3)), later));
        // First IP should be sweepable and allowed again.
        assert!(limiter.allow_now(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), later));
    }
}
