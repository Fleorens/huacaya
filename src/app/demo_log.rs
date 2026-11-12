//! The live demo cycle's raw log lines (T11), separated out so `demo.rs` stays under 400 lines.
//!
//! Each line is `<ts> aibox-flo llama-server[1194]: <body>`; `demo::log_lines` builds and classifies
//! them. Times, task number and the loaded model id are the only things cycle-dependent.

use crate::app::demo::LOADED_ID;

/// The `(event time, raw line)` pairs of cycle `n`, in time order then in the order the sheet lists.
pub(crate) fn cycle_lines(n: u64) -> Vec<(f64, String)> {
    let task = 9000 + n;
    let m = LOADED_ID;
    let mut out = Vec::new();
    let add = |out: &mut Vec<(f64, String)>, c: f64, body: String| out.push((c, body));

    add(
        &mut out,
        8.0,
        format!("90.25.429.894 I srv  proxy_reques: proxying request to model {m} on port 35335"),
    );
    add(
        &mut out,
        8.0,
        format!(
            "[35335] 60.27.289.353 I slot launch_slot_: id  0 | task {task} | processing task, is_child = 0"
        ),
    );
    for c in 9..=11 {
        let k = c - 8;
        add(
            &mut out,
            c as f64,
            format!(
                "[35335] 60.27.289.353 I slot update_slots: id  0 | task {task} | prompt processing, n_tokens = {}, progress = {:.2}, t = {}.00 s / 890.00 tokens per second",
                2048 * k,
                0.25 * k as f64,
                k
            ),
        );
    }
    add(
        &mut out,
        12.0,
        format!(
            "[35335] 60.27.289.353 I slot print_timing: id  0 | task {task} | prompt eval time =    9195.00 ms /  8192 tokens (    1.12 ms per token,   891.00 tokens per second)"
        ),
    );
    let mut c = 15;
    while c <= 30 {
        add(
            &mut out,
            c as f64,
            format!(
                "[35335] 60.27.289.353 I slot print_timing: id  0 | task {task} | n_gen = {}, tg =  52.10 t/s, tg_3s =  53.00 t/s",
                52 * (c - 12)
            ),
        );
        c += 3;
    }
    add(
        &mut out,
        32.0,
        format!(
            "[35335] 60.27.289.353 I slot print_timing: id  0 | task {task} |        eval time =   20000.00 ms /  1040 tokens (   19.23 ms per token,    52.00 tokens per second)"
        ),
    );
    add(
        &mut out,
        32.0,
        format!(
            "[35335] 60.27.289.353 I slot print_timing: id  0 | task {task} |       total time =   29195.00 ms /  9232 tokens"
        ),
    );
    add(
        &mut out,
        32.0,
        format!(
            "[35335] 60.27.289.353 I slot print_timing: id  0 | task {task} | draft acceptance = 0.70000 (  728 accepted /  1040 generated), mean len =  2.80"
        ),
    );
    add(
        &mut out,
        32.0,
        format!(
            "[35335] 60.27.289.353 I slot      release: id  0 | task {task} | stop processing: n_tokens = 9232, truncated = 0"
        ),
    );
    add(
        &mut out,
        32.0,
        "90.25.429.894 I srv  log_server_r: request: POST /v1/chat/completions 192.168.1.42 200".into(),
    );
    add(&mut out, 32.0, "[35335] 60.27.289.353 I srv  update_slots: all slots are idle".into());
    out
}

/// The full journal line: `<ts> aibox-flo llama-server[1194]: <body>`.
pub(crate) fn raw_line(e: f64, body: &str) -> String {
    format!("{} aibox-flo llama-server[1194]: {}", timestamp(e), body)
}

/// The ISO timestamp of event time `e`: `36_000 + e` seconds of the day (modulo 86 400), on 2026-11-12.
fn timestamp(e: f64) -> String {
    let sec = (36_000 + e.floor() as u64) % 86_400;
    format!("2026-11-12T{:02}:{:02}:{:02}.000000+01:00", sec / 3600, (sec % 3600) / 60, sec % 60)
}
