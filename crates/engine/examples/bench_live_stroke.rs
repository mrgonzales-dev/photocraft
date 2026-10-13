//! Time the interactive brush path the way the canvas drives it: a LiveStroke fed a few points
//! per frame, then the commit that takes its pixels. Prints per-stage timings.
//!
//! cargo run --release -p photocraft-engine --example bench_live_stroke -- [w] [h] [depth] [size] [points]

use photocraft_engine::Session;
use photocraft_engine::brush_cmds::LiveStroke;
use photocraft_engine::paint::StrokePoint;
use serde_json::{Value, json};
use std::time::Instant;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn exec(s: &mut Session, id: &str, p: Value) {
    if let Err(e) = s.execute(id, p) {
        eprintln!("{id}: {e}");
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let w = a.get(1).and_then(|v| v.parse().ok()).unwrap_or(8000u32);
    let h = a.get(2).and_then(|v| v.parse().ok()).unwrap_or(6000u32);
    let depth = a.get(3).and_then(|v| v.parse().ok()).unwrap_or(8u32);
    let size = a.get(4).and_then(|v| v.parse().ok()).unwrap_or(100f32);
    let n_pts = a.get(5).and_then(|v| v.parse().ok()).unwrap_or(600usize);

    let mode = a.get(6).map(|v| v.as_str()).unwrap_or("rgb");
    let mut s = Session::new();
    exec(&mut s, "file.new", json!({"width": w, "height": h, "depth": depth, "mode": mode, "background": "white"}));
    exec(&mut s, "layer.new.layer", json!({}));
    exec(&mut s, "tools.setColors", json!({"foreground": "#2030c0"}));
    exec(&mut s, "tools.setBrush", json!({"brush": {"size": size, "hardness": 0.5}}));

    // The stroke's input: a diagonal with a wiggle, like a real drag.
    let pts: Vec<Value> = (0..n_pts)
        .map(|i| {
            let t = i as f64 / n_pts as f64;
            json!([w as f64 * (0.2 + 0.6 * t), h as f64 * (0.4 + 0.2 * t) + (t * 40.0).sin() * 60.0, 1.0])
        })
        .collect();
    let params = json!({"points": pts, "size": size, "hardness": 0.5});

    // --- begin ---
    let t = Instant::now();
    let mut live = match LiveStroke::begin_with(&s, "paint.stroke", &json!({"points": [pts[0].clone()], "size": size, "hardness": 0.5})) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("begin: {e}");
            return;
        }
    };
    let begin_ms = ms(t);
    println!("begin: {begin_ms:.2} ms");

    // --- per-frame pushes: ~6 points each, like a 100-sample frame budget ---
    let mut push_times = Vec::new();
    let per = 6;
    let mut i = 1;
    while i < n_pts {
        let chunk: Vec<StrokePoint> = pts[i..(i + per).min(n_pts)]
            .iter()
            .map(|v| StrokePoint::new(v[0].as_f64().unwrap_or(0.0), v[1].as_f64().unwrap_or(0.0), 1.0))
            .collect();
        let t = Instant::now();
        let _ = live.push(&chunk);
        push_times.push(ms(t));
        i += per;
    }
    push_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p50 = push_times[push_times.len() / 2];
    let p95 = push_times[(push_times.len() * 95 / 100).min(push_times.len() - 1)];
    let max = push_times.last().copied().unwrap_or(0.0);
    println!("push (n={}): p50 {p50:.2} ms  p95 {p95:.2} ms  max {max:.2} ms", push_times.len());

    // --- commit: offer + paint.stroke (what the release frame does) ---
    let seed = live.seed;
    let mut cp = params.clone();
    cp["seed"] = json!(seed);
    let t = Instant::now();
    let _ = s.offer_live_stroke(live);
    println!("offer_live_stroke: {:.2} ms", ms(t));
    let t = Instant::now();
    let r = s.execute("paint.stroke", cp);
    println!("commit paint.stroke: {:.2} ms (fromLive={})", ms(t), r.as_ref().map(|v| v["fromLive"].clone()).unwrap_or(Value::Null));

    // --- the display refresh the same frame does ---
    let st = s.active().expect("doc");
    let dmg = st.last_damage.unwrap_or(st.doc.bounds());
    let t = Instant::now();
    let _buf = photocraft_compose::render(&st.doc, dmg);
    println!("cpu composite of damage {dmg:?}: {:.2} ms", ms(t));
}
