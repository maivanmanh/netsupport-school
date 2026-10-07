//! Local synthetic AU probe using the production sender. No capture or decoder.
#[path = "../fanout.rs"]
mod fanout;
#[path = "../udp_sender.rs"]
mod udp_sender;

use anyhow::{ensure, Result};
use clap::Parser;
use fanout::{Fanout, FanoutConfig, FanoutJob};
use serde_json::json;
use std::{
    net::{Ipv4Addr, UdpSocket},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use udp_sender::AggregatePacer;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 35)]
    receivers: usize,
    #[arg(long, default_value_t = 4)]
    workers: usize,
    #[arg(long, default_value_t = 400)]
    pacing_mbps: u32,
    #[arg(long, default_value_t = 60)]
    frames: u32,
    #[arg(long, default_value_t = 30)]
    fps: u32,
    #[arg(long, default_value_t = 25000)]
    normal_bytes: usize,
    #[arg(long, default_value_t = 361003)]
    idr_bytes: usize,
    #[arg(long, default_value_t = 120)]
    idr_every: u32,
    #[arg(long, default_value_t = 13)]
    encode_ms: u64,
    #[arg(long, default_value_t = 120)]
    au_deadline_ms: u64,
    #[arg(long, default_value_t = 300)]
    idr_deadline_ms: u64,
    #[arg(long, default_value_t = 0)]
    auxiliary_packets: u32,
    #[arg(long)]
    connected: bool,
    #[arg(long)]
    output: PathBuf,
}

#[allow(clippy::too_many_lines, clippy::cast_precision_loss)] // Standalone bounded diagnostic orchestration; byte counters fit exact f64 range.
fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(
        (1..=40).contains(&args.receivers),
        "receivers must be 1..40"
    );
    ensure!((1..=16).contains(&args.workers), "workers must be 1..16");
    ensure!(
        (1..=3000).contains(&args.frames) && (1..=30).contains(&args.fps),
        "invalid duration/cadence"
    );
    ensure!(
        (16..=2_000_000).contains(&args.normal_bytes) && (16..=2_000_000).contains(&args.idr_bytes),
        "invalid AU size"
    );
    ensure!(
        args.idr_every > 0 && args.encode_ms <= 1000 && args.auxiliary_packets <= 10000,
        "invalid probe load"
    );
    let stop = Arc::new(AtomicBool::new(false));
    let mut targets = Vec::new();
    let mut sinks = Vec::new();
    for id in 1..=args.receivers {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        socket.set_read_timeout(Some(Duration::from_millis(20)))?;
        let std::net::SocketAddr::V4(address) = socket.local_addr()? else {
            unreachable!("IPv4 bound socket")
        };
        targets.push((id as u64, address));
        let stop = Arc::clone(&stop);
        sinks.push(thread::spawn(move || {
            let mut seen =
                std::collections::HashMap::<u32, (std::collections::HashSet<u32>, u32)>::new();
            let mut packets = 0_u64;
            let mut duplicates = 0_u64;
            let mut buffer = [0_u8; 1300];
            while !stop.load(Ordering::Acquire) {
                match socket.recv_from(&mut buffer) {
                    Ok((n, _)) if n >= 16 => {
                        packets += 1;
                        let frame = u32::from_be_bytes(buffer[0..4].try_into().unwrap());
                        let index = u32::from_be_bytes(buffer[4..8].try_into().unwrap());
                        let count = u32::from_be_bytes(buffer[8..12].try_into().unwrap());
                        let entry = seen
                            .entry(frame)
                            .or_insert_with(|| (std::collections::HashSet::new(), count));
                        // Ordered per-recipient sends; repeated indexes are diagnostic.
                        if !entry.0.insert(index) {
                            duplicates += 1;
                        }
                    }
                    Ok(_) => {}
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(e) => return Err(e),
                }
            }
            let complete = seen
                .values()
                .filter(|(received, count)| received.len() == *count as usize)
                .count();
            Ok(
                json!({"id": id, "packets": packets, "complete_aus": complete,
                "partial_aus": seen.len() - complete, "duplicates": duplicates}),
            )
        }));
    }
    let shared = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
    shared.set_nonblocking(true)?;
    let pacer = Arc::new(AggregatePacer::new(args.pacing_mbps));
    let fan = Fanout::start_paced(
        &shared,
        FanoutConfig {
            workers: args.workers,
            connected: args.connected,
            pacing_mbps: args.pacing_mbps,
            max_au_age: Duration::from_millis(args.au_deadline_ms),
            max_keyframe_age: Duration::from_millis(args.idr_deadline_ms),
            send_buffer_bytes: 262_144,
        },
        &pacer,
    )?;
    // Auxiliary traffic uses the same pacer, as repair/pointer do in production.
    let aux_sink = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
    let std::net::SocketAddr::V4(aux_address) = aux_sink.local_addr()? else {
        unreachable!("IPv4 bound socket")
    };
    let aux_stop = Arc::clone(&stop);
    aux_sink.set_read_timeout(Some(Duration::from_millis(20)))?;
    let aux_reader = thread::spawn(move || {
        let mut n = 0;
        let mut buffer = [0; 1300];
        while !aux_stop.load(Ordering::Acquire) {
            if aux_sink.recv_from(&mut buffer).is_ok() {
                n += 1;
            }
        }
        n
    });
    let started = Instant::now();
    let tick = Duration::from_secs_f64(1.0 / f64::from(args.fps));
    let mut rows = Vec::new();
    let mut aux_sent = 0_u64;
    for frame in 0..args.frames {
        let due = started + tick * frame;
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }
        let before_wait = Instant::now();
        fan.wait_idle(&stop);
        let wait_ms = before_wait.elapsed().as_secs_f64() * 1000.0;
        thread::sleep(Duration::from_millis(args.encode_ms));
        let recovery = fan.stats.recovery_requested.swap(false, Ordering::AcqRel);
        let idr = frame % args.idr_every == 0 || recovery;
        let bytes = if idr {
            args.idr_bytes
        } else {
            args.normal_bytes
        };
        let count = bytes.div_ceil(1200);
        let mut packets = Vec::with_capacity(count);
        for index in 0..count {
            let mut p = vec![0_u8; (bytes - index * 1200).clamp(16, 1200)];
            p[0..4].copy_from_slice(&frame.to_be_bytes());
            p[4..8].copy_from_slice(&u32::try_from(index)?.to_be_bytes());
            p[8..12].copy_from_slice(&u32::try_from(count)?.to_be_bytes());
            packets.push(p);
        }
        let send_started = Instant::now();
        fan.send(FanoutJob {
            is_keyframe: idr,
            packets,
            targets: targets.clone(),
        });
        for _ in 0..args.auxiliary_packets {
            let deadline = Instant::now() + Duration::from_millis(5);
            if pacer.wait(1200, deadline).is_ok() && shared.send_to(&[0; 1200], aux_address).is_ok()
            {
                aux_sent += 1;
            }
        }
        fan.wait_idle(&stop);
        rows.push(
            json!({"frame": frame, "idr": idr, "bytes": bytes, "pre_encode_wait_ms": wait_ms,
            "send_complete_ms": send_started.elapsed().as_secs_f64() * 1000.0,
            "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0}),
        );
    }
    let elapsed = started.elapsed();
    let _stages = fan.take_stages();
    let stat = &fan.stats;
    let sent = stat.sent_packets.load(Ordering::Relaxed);
    let sent_bytes = stat.sent_bytes.load(Ordering::Relaxed);
    let sender = json!({"sent_packets": sent, "sent_bytes": sent_bytes,
        "send_errors": stat.send_errors.load(Ordering::Relaxed),
        "abandoned": stat.abandoned_packets.load(Ordering::Relaxed),
        "reference_skips": stat.dependency_skipped_packets.load(Ordering::Relaxed),
        "au_deadline_drops": stat.au_deadline_drops.load(Ordering::Relaxed),
        "would_block": stat.would_block.load(Ordering::Relaxed),
        "pacer_denied": pacer.denied.load(Ordering::Relaxed),
        "wire_budget_mbps_estimate": ((sent_bytes + sent * 80 + aux_sent * 1280) as f64 * 8.0) / elapsed.as_secs_f64() / 1e6});
    drop(fan);
    thread::sleep(Duration::from_millis(100));
    stop.store(true, Ordering::Release);
    let received: Vec<_> = sinks
        .into_iter()
        .map(|h| h.join().expect("sink panic"))
        .collect::<std::io::Result<_>>()?;
    let auxiliary_received = aux_reader.join().expect("aux sink panic");
    let report = json!({"scope": "same-host loopback synthetic AU sizes; no RTP decode, NIC or optical acceptance",
        "receivers": args.receivers, "workers": args.workers, "pacing_mbps": args.pacing_mbps, "stream_ceiling_bps": pacer.bitrate_ceiling(args.receivers),
        "encode_ms_simulated": args.encode_ms, "frames": args.frames, "elapsed_s": elapsed.as_secs_f64(),
        "submission_fps": f64::from(args.frames) / elapsed.as_secs_f64(),
        "auxiliary_sent": aux_sent, "auxiliary_received": auxiliary_received,
        "sender": sender, "sinks": received, "aus": rows});
    if let Some(parent) = args.output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&args.output, serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string(&report["sender"])?);
    println!("report={}", args.output.display());
    Ok(())
}
