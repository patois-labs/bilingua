//! One large generated message (over 5 MB of text), extracted and merged. Run `cargo test --release -p
//! bilingua-email --test speed -- --nocapture` for the timings. Set `EMAIL_SPEED_OUT` to a path to also save the
//! message, so the LocOS filter can be timed on the same bytes (`tools/speed_locos.py`).

mod common;
use bilingua_email::Email;
use bilingua_model::*;
use common::*;
use std::time::Instant;

pub fn generate() -> Vec<u8> {
    let words = ["harbor", "lantern", "bread", "window", "morning", "garden", "ticket", "river", "paper", "market"];
    let mut plain = String::new();
    for n in 0..30_000usize {
        let w = |k: usize| words[(n * 7 + k) % words.len()];
        plain.push_str(&format!("Open the {} {} near the {} at noon and bring {} for {} sentence {n} of the guide, in full, caf\u{e9}.\n\n", w(1), w(2), w(3), w(4), w(5)));
    }
    let mut html = String::from("<html><head><meta charset=\"utf-8\"></head><body>");
    for n in 0..12_000usize {
        let w = |k: usize| words[(n * 3 + k) % words.len()];
        html.push_str(&format!("<p>Open the <b>{}</b> {} near the <a href=\"https://example.com/{n}\">{}</a> and bring {} for sentence {n} of the guide.</p>\n", w(1), w(2), w(3), w(4)));
    }
    html.push_str("</body></html>\n");
    let qp_plain = plain.replace('\u{e9}', "=C3=A9");
    format!(
        "From: Ann <ann@example.com>\r\nTo: Bob <bob@example.org>\r\nSubject: Large guide\r\nMIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=\"speed\"\r\n\r\n--speed\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\n{}--speed\r\nContent-Type: text/html; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{}--speed--\r\n",
        qp_plain.replace('\n', "\r\n"),
        b64_crlf(html.as_bytes())
    )
    .into_bytes()
}

#[test]
fn large_message_round_trips_and_is_timed() {
    let bytes = generate();
    assert!(bytes.len() >= 5_000_000, "only {} bytes", bytes.len());
    if let Ok(path) = std::env::var("EMAIL_SPEED_OUT") {
        std::fs::write(path, &bytes).unwrap();
    }
    let t0 = Instant::now();
    let mut d = Email.extract(&bytes, &opts(Some("de"))).unwrap();
    let t_extract = t0.elapsed();
    assert!(d.units.len() > 40_000, "{} units", d.units.len());
    let t1 = Instant::now();
    let same = Email.merge(&d).unwrap();
    let t_merge = t1.elapsed();
    assert_eq!(same, bytes);
    pseudo(&mut d);
    let t2 = Instant::now();
    let out = Email.merge(&d).unwrap();
    let t_merge_tr = t2.elapsed();
    let back = Email.extract(&out, &opts(None)).unwrap();
    assert_eq!(back.units.len(), d.units.len());
    println!(
        "email speed: {} bytes, {} units; extract {:?}, untouched merge {:?}, translated merge {:?}; extract+merge {:?}",
        bytes.len(),
        d.units.len(),
        t_extract,
        t_merge,
        t_merge_tr,
        t_extract + t_merge_tr
    );
}
