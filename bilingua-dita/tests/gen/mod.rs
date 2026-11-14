//! The large generated DITA topic of the speed test and `examples/dita_dump.rs gen` (made-up text).

pub fn topic(min_bytes: usize) -> String {
    let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE topic PUBLIC \"-//OASIS//DTD DITA Topic//EN\" \"topic.dtd\">\n<!-- Generated Lumen Bakery topic (made up) -->\n<topic id=\"big\" xml:lang=\"en-US\">\n  <title>Lumen Bakery catalog</title>\n  <body>\n",
    );
    let mut i = 0usize;
    while s.len() < min_bytes {
        let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
        s.push_str(&format!(
            "    <section id=\"s{i}\"><title>Our {} {}</title>\n      <p>A <b>{}</b> loaf, baked <i>{}</i> every day &amp; sold with <codeph>SKU-{i}</codeph> code.<fn>Caf\u{e9} \u{65e5}\u{672c} {}.</fn></p>\n      <p conref=\"shared.dita#shared/p{i}\"/>\n      <p>Press <uicontrol>Order</uicontrol> and see <xref href=\"o.dita\">{}</xref>.<indexterm>{}</indexterm></p>\n      <image href=\"i{i}.png\" alt=\"A {} loaf\"/>\n    </section>\n",
            w(1), w(2), w(3), w(4), w(5), w(6), w(7), w(8)
        ));
        i += 1;
    }
    s.push_str("  </body>\n</topic>\n");
    s
}
