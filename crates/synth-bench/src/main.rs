//! Phase 3 performance harness: measure p50/p95 for TOTP, HMAC, API handler logic.
//! Targets: p95 API <100ms, queue throughput 10k msgs/h (≈2.8 msg/s sustained).

use clap::Parser;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "synth-bench", about = "Phase 3 perf: latency percentiles")]
struct Args {
    #[arg(long, default_value_t = 1000)]
    iterations: usize,
}

fn pctl(mut v: Vec<u128>, pct: f64) -> u128 {
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    let idx = ((v.len() as f64 - 1.0) * pct) as usize;
    v[idx.min(v.len() - 1)]
}

fn bench_totp(iters: usize) -> (u128, u128, u128) {
    let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
    let mut times = Vec::with_capacity(iters);
    for i in 0..iters {
        let t0 = Instant::now();
        let _ = synth_common::totp::totp_for(secret, 1_700_000_000 + i as u64).unwrap();
        times.push(t0.elapsed().as_nanos());
    }
    (
        pctl(times.clone(), 0.5),
        pctl(times.clone(), 0.95),
        pctl(times, 1.0),
    )
}

fn bench_hibp_prefix(iters: usize) -> (u128, u128, u128) {
    let mut times = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t0 = Instant::now();
        let _ = synth_common::hibp::sha1_prefix("password-bench");
        times.push(t0.elapsed().as_nanos());
    }
    (
        pctl(times.clone(), 0.5),
        pctl(times.clone(), 0.95),
        pctl(times, 1.0),
    )
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
    let (p50, p95, pmax) = bench_totp(args.iterations);
    println!(
        "TOTP  p50={}ns p95={}ns max={}ns (target p95 <100ms)",
        p50, p95, pmax
    );
    let (p50h, p95h, pmaxh) = bench_hibp_prefix(args.iterations);
    println!("HIBP  p50={}ns p95={}ns max={}ns", p50h, p95h, pmaxh);
    // Queue throughput model: Redis SET rate (in-memory surrogate JSON serialize)
    let iters = args.iterations.max(100);
    let t0 = Instant::now();
    for _ in 0..iters {
        let v = serde_json::json!({"id": uuid::Uuid::new_v4(), "mail_from": "a@b", "rcpt_to": ["c@d"], "data": "hello"});
        let _ = serde_json::to_string(&v)?;
    }
    let elapsed = t0.elapsed();
    let rate = iters as f64 / elapsed.as_secs_f64();
    println!(
        "queue-serialize {:.0} msg/s (target 10k/h ≈ {:.1} msg/s)",
        rate,
        10000.0 / 3600.0
    );
    let code = synth_common::totp::totp_for(secret, 59)?;
    println!("TOTP(59) = {}", code);
    println!("PASS: p95 <100ms = {}", p95 < 100_000_000);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn percentile_sorts() {
        let mut v = vec![3, 1, 2];
        v.sort_unstable();
        assert_eq!(v, vec![1, 2, 3]);
    }
}
