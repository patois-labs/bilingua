"""Writes the made-up XML corpus into bilingua/oracle/corpus/xml/ (stdlib only). Everything is invented.

    python gen_corpus.py

The output is committed; run this only when the corpus changes. Deterministic.
"""

from pathlib import Path

OUT = Path(__file__).resolve().parents[3] / "oracle" / "corpus" / "xml"

LANGS = [
    ("en", "Welcome to Lumen Bakery. Fresh bread is ready at six every morning."),
    ("es", "Bienvenido a Panadería Lumen. El pan fresco está listo a las seis de cada mañana."),
    ("fr", "Bienvenue à la Boulangerie Lumen. Le pain frais est prêt à six heures chaque matin."),
    ("it", "Benvenuto al Forno Lumen. Il pane fresco è pronto ogni mattina alle sei."),
    ("de", "Willkommen in der Bäckerei Lumen. Frisches Brot ist jeden Morgen um sechs Uhr fertig."),
    ("nl", "Welkom bij Bakkerij Lumen. Vers brood staat elke ochtend om zes uur klaar."),
    ("pt", "Bem-vindo à Padaria Lumen. O pão fresco fica pronto às seis horas todas as manhãs."),
    ("ja", "ルーメン・ベーカリーへようこそ。焼きたてのパンは毎朝六時にご用意しています。"),
    ("zh", "欢迎光临流明面包店。新鲜面包每天早上六点出炉。"),
    ("ko", "루멘 베이커리에 오신 것을 환영합니다. 갓 구운 빵은 매일 아침 여섯 시에 준비됩니다."),
    ("ar", "مرحبا بكم في مخبز لومن. الخبز الطازج جاهز كل صباح في السادسة."),
]


def w(name: str, data) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / name).write_bytes(data.encode("utf-8") if isinstance(data, str) else data)


def esc(s: str) -> str:
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


# 1. plain XML with ITS rules in the document, inline markup, CDATA, comments, a processing instruction
w(
    "bakery_menu.xml",
    """<?xml version="1.0" encoding="UTF-8"?>
<!-- Lumen Bakery menu (made up) -->
<?render-hint compact?>
<menu xmlns:its="http://www.w3.org/2005/11/its" its:version="2.0" xml:lang="en">
  <its:rules version="2.0">
    <its:translateRule selector="//price" translate="no"/>
    <its:translateRule selector="//item/@label" translate="yes"/>
    <its:translateRule selector="//item/@sku" translate="no"/>
    <its:withinTextRule selector="//b | //i | //br" withinText="yes"/>
    <its:locNoteRule selector="//desc">
      <its:locNote>Keep under 90 characters.</its:locNote>
    </its:locNoteRule>
    <its:locNoteRule selector="//name" locNotePointer="../hint"/>
    <its:preserveSpaceRule selector="//recipe" space="preserve"/>
  </its:rules>
  <item sku="B-100" label="Sourdough loaf">
    <name>Sourdough</name>
    <hint>Product name, do not shorten.</hint>
    <desc>A <b>crusty</b> loaf, baked <i>fresh</i> every day.<br/>Order before noon.</desc>
    <price currency="EUR">4.50</price>
  </item>
  <item sku="B-200" label="Croissant">
    <name>Croissant</name>
    <hint>Plural is fine.</hint>
    <desc>Butter &amp; flour, nothing else. Serves 1 &lt; 2 people.</desc>
    <price currency="EUR">2.10</price>
    <recipe>Mix 500 g flour
    with 300 ml water.
      Rest overnight.</recipe>
  </item>
  <item sku="B-300" label="Seed bar">
    <name>Seed bar</name>
    <hint>Snack.</hint>
    <desc><![CDATA[Oats, <seeds> & honey.]]></desc>
    <price currency="EUR">1.90</price>
    <!-- seasonal -->
    <seasonal its:translate="no">Autumn only</seasonal>
  </item>
</menu>
""",
)

# 2. one document, eleven languages as ordinary sections (not a multilingual layout: single group)
body = "".join(f'  <section code="{c}">\n    <p>{esc(t)}</p>\n  </section>\n' for c, t in LANGS)
w("bakery_languages.xml", f'<?xml version="1.0" encoding="UTF-8"?>\n<welcome>\n{body}</welcome>\n')

# 3. multilingual XML, language attribute siblings, every language, source en
entries = []
for n in range(1, 5):
    rows = "".join(f'    <text xml:lang="{c}">{esc(t)} ({n})</text>\n' for c, t in LANGS if not (n == 3 and c == "ar"))
    entries.append(f'  <entry id="e{n}">\n{rows}  </entry>\n')
w("bakery_multilingual_attr.xml", '<?xml version="1.0" encoding="UTF-8"?>\n<strings xml:lang="en">\n' + "".join(entries) + "</strings>\n")

# 4. multilingual XML, one element per language, with inline markup and a missing language
rows = []
for n, (a, b) in enumerate([("Open", "Ouvrir"), ("Save <b>all</b>", "Tout enregistrer"), ("Close", "Fermer")], 1):
    rows.append(f'  <string key="k{n}">\n    <en>{a}</en>\n    <fr>{b}</fr>\n    <ja>{"開く" if n == 1 else ""}</ja>\n    <ar>{"فتح" if n == 1 else ""}</ar>\n  </string>\n')
w("bakery_multilingual_element.xml", '<?xml version="1.0" encoding="UTF-8"?>\n<strings>\n' + "".join(rows) + "</strings>\n")

# 5. multilingual by XPath rule (processing instruction)
w(
    "bakery_multilingual_xpath.xml",
    """<?xml version="1.0" encoding="UTF-8"?>
<?loc-multilingual mode="xpath" source="en_US" lang-attr="loc" source-xpath="//msg/v[@loc='{lang}']" target-xpath="../v[@loc='{lang}']"?>
<messages>
  <msg name="open"><v loc="en_US">Open the door</v><v loc="de_DE">Tür öffnen</v></msg>
  <msg name="close"><v loc="en_US">Close the <b>door</b></v></msg>
</messages>
""",
)

# 6. DITA topic (sniffed from the DOCTYPE)
w(
    "bakery_topic.xml",
    """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE task PUBLIC "-//OASIS//DTD DITA Task//EN" "task.dtd">
<task id="bake" xml:lang="en-US">
  <title>Baking a <ph>sourdough</ph> loaf</title>
  <shortdesc>Bake the loaf at the right heat.</shortdesc>
  <taskbody>
    <steps>
      <step><cmd>Heat the oven to <codeph>230 C</codeph>.</cmd></step>
      <step><cmd>Press <uicontrol>Start</uicontrol> and wait.</cmd><info>Do not open the door.<indexterm>baking</indexterm></info></step>
    </steps>
    <example><codeblock xml:space="preserve">oven --temp 230
  --steam on</codeblock><image href="oven.png" alt="The oven dial"/></example>
  </taskbody>
</task>
""",
)

# 7. DocBook 5
w(
    "bakery_docbook.xml",
    """<?xml version="1.0" encoding="UTF-8"?>
<article xmlns="http://docbook.org/ns/docbook" version="5.0" xml:lang="en">
  <info><title>Bakery guide</title></info>
  <section>
    <title>Starter</title>
    <para>Feed the <emphasis>starter</emphasis> twice a day.<footnote><para>Use rye flour.</para></footnote></para>
    <programlisting>make starter</programlisting>
  </section>
</article>
""",
)

# 8. edge cases: entities, character references, namespaces, empty elements, comments in text, a CDATA-only value
w(
    "edge.xml",
    """<?xml version='1.0'?>
<!DOCTYPE note SYSTEM "note.dtd">
<note xmlns:x="urn:lumen" x:id="n1">
  <to>Anna &amp; Ben</to>
  <body>Fish &#38; chips &#x263A; for &company; at &lt;noon&gt;</body>
  <mixed>one <!-- gone --> two <?pi data?> three</mixed>
  <empty/>
  <nbsp>a&#160;b</nbsp>
  <cdata><![CDATA[1 < 2]]></cdata>
  <deep><a><b><c>deep text</c></b></a></deep>
  <x:ns>namespaced text</x:ns>
  <attrs title="Don't &quot;panic&quot;" other='single &amp; quoted'>content</attrs>
  <blank>   </blank>
  <tail>text<tail2>inner</tail2>after</tail>
</note>
""",
)

# 9. encodings
LAT = "Boulangerie Lumen : pâtisserie crème brûlée, café noir. Prix en €."
w("bakery_windows1252.xml", f'<?xml version="1.0" encoding="windows-1252"?>\n<doc>\n  <p>{esc(LAT)}</p>\n  <p>Œuvre ‘quoted’ “text”</p>\n</doc>\n'.encode("cp1252"))
w("bakery_shiftjis.xml", '<?xml version="1.0" encoding="Shift_JIS"?>\n<doc>\n  <p>ルーメン・ベーカリーへようこそ。</p>\n  <p>焼きたてのパンは毎朝六時です。</p>\n</doc>\n'.encode("shift_jis"))
w("bakery_gbk.xml", '<?xml version="1.0" encoding="GBK"?>\n<doc>\n  <p>欢迎光临流明面包店。</p>\n  <p>新鲜面包每天早上六点出炉。</p>\n</doc>\n'.encode("gbk"))
w("bakery_euckr.xml", '<?xml version="1.0" encoding="EUC-KR"?>\n<doc>\n  <p>루멘 베이커리에 오신 것을 환영합니다.</p>\n  <p>갓 구운 빵은 매일 아침 여섯 시에 준비됩니다.</p>\n</doc>\n'.encode("euc-kr"))
w("bakery_win1256.xml", '<?xml version="1.0" encoding="windows-1256"?>\n<doc>\n  <p>مرحبا بكم في مخبز لومن.</p>\n  <p>الخبز الطازج جاهز كل صباح.</p>\n</doc>\n'.encode("cp1256"))
u16 = '<?xml version="1.0" encoding="UTF-16"?>\r\n<doc>\r\n  <p>Welcome to Lumen Bakery</p>\r\n  <p>ルーメン・ベーカリー</p>\r\n</doc>\r\n'
w("bakery_utf16le.xml", b"\xff\xfe" + u16.encode("utf-16-le"))
w("bakery_utf8_bom_crlf.xml", b"\xef\xbb\xbf" + '<?xml version="1.0" encoding="UTF-8"?>\r\n<doc>\r\n  <p>Fresh bread</p>\r\n  <p>Café noir</p>\r\n</doc>\r\n'.encode("utf-8"))
print("written to", OUT)
