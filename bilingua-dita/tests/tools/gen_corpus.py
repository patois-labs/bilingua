"""Writes the made-up DITA and DocBook corpus into bilingua/oracle/corpus/dita/ (stdlib only). Everything is invented.

    python gen_corpus.py

The output is committed; run this only when the corpus changes. Deterministic.
"""

from pathlib import Path

OUT = Path(__file__).resolve().parents[3] / "oracle" / "corpus" / "dita"

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


def w(name: str, data: str) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / name).write_bytes(data.encode("utf-8"))


# 1. a DITA task: code protected, conref / conkeyref / keyref left alone, translate="no", inline codes, nested
#    footnote and index terms, image alt text, a preserved code block, an entity from the external DTD
w(
    "lumen_starter.dita",
    """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE task PUBLIC "-//OASIS//DTD DITA Task//EN" "task.dtd">
<task id="feed-starter" xml:lang="en-US">
  <title>Feeding the <ph>sourdough</ph> starter</title>
  <shortdesc>Feed the starter <b>twice</b> a day, at <i>eight</i> and at <i>eight</i>.</shortdesc>
  <!-- writer: keep the times in words -->
  <prolog><metadata><keywords><indexterm>starter<indexterm>feeding</indexterm></indexterm></keywords></metadata></prolog>
  <taskbody>
    <prereq><p conref="shared.dita#shared/hygiene"/><p conkeyref="safety/gloves"/></prereq>
    <context><p>Use the <codeph>feed --grams {0}</codeph> script on the <keyword keyref="mixer"/> or see <xref href="mixing.dita">Mixing</xref>.<fn>Any scale works.</fn></p>
      <p translate="no">Batch code LB-2026-11</p>
      <draft-comment>Ask the head baker about rye.</draft-comment>
      <p>Press <uicontrol>Start</uicontrol> &amp; wait &lt;10&gt; minutes for &product;.</p>
    </context>
    <steps>
      <step><cmd>Discard half of the <term>starter</term>.</cmd></step>
      <step><cmd>Add 100 g of flour and 100 g of water.</cmd><info><note type="tip">Use water at <ph keyref="temp">27 °C</ph>.</note></info></step>
      <step><cmd>Stir, cover and leave it at room temperature.</cmd></step>
    </steps>
    <example><codeblock xml:space="preserve">feed --grams 100
  --water 100</codeblock><image href="starter.png" alt="A jar of bubbly starter"/></example>
  </taskbody>
</task>
""",
)

# 2. a DITA concept in Japanese (root carries the architecture namespace)
w(
    "lumen_concept_ja.dita",
    """<?xml version="1.0" encoding="UTF-8"?>
<concept xmlns:ditaarch="http://dita.oasis-open.org/architecture/2005/" ditaarch:DITAArchVersion="1.3" id="about" xml:lang="ja-JP">
  <title>ルーメン・ベーカリーについて</title>
  <shortdesc>焼きたてのパンは毎朝<b>六時</b>にご用意しています。</shortdesc>
  <conbody>
    <p>ご予約は<xref href="order.dita">注文ページ</xref>から。<codeph>ORDER-{id}</codeph>をお控えください。</p>
    <ul><li>天然酵母パン</li><li>クロワッサン</li><li translate="no">SKU-JP-07</li></ul>
    <p>アレルギー情報は<uicontrol>表示</uicontrol>を押してください。</p>
  </conbody>
</concept>
""",
)

# 3. a DITA map: title and navtitle attributes, topicmeta, key definitions
w(
    "lumen_guide.ditamap",
    """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE map PUBLIC "-//OASIS//DTD DITA Map//EN" "map.dtd">
<map title="Lumen Bakery Guide" xml:lang="en-US">
  <topicmeta><navtitle>Lumen Bakery Guide</navtitle><shortdesc>Everything about our bread.</shortdesc></topicmeta>
  <keydef keys="mixer" href="mixer.dita"><topicmeta><keywords><keyword>Spiral mixer</keyword></keywords></topicmeta></keydef>
  <topicref href="lumen_starter.dita" navtitle="Feeding the starter"/>
  <topicref href="shaping.dita" navtitle='Shaping &amp; proofing'>
    <topicref href="baking.dita"><topicmeta><navtitle>Baking</navtitle></topicmeta></topicref>
  </topicref>
  <topicref keyref="mixer"/>
</map>
""",
)

# 4. a bookmap
w(
    "lumen_handbook.bookmap",
    """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE bookmap PUBLIC "-//OASIS//DTD DITA BookMap//EN" "bookmap.dtd">
<bookmap id="handbook">
  <booktitle><booklibrary>Lumen Library</booklibrary><mainbooktitle>The Baker's Handbook</mainbooktitle><booktitlealt>Second edition</booktitlealt></booktitle>
  <bookmeta><author>Lumen Bakery</author><bookrights><copyrfirst><year>2026</year></copyrfirst></bookrights></bookmeta>
  <frontmatter><notices href="notices.dita" navtitle="Notices"/></frontmatter>
  <chapter href="lumen_starter.dita" navtitle="Starters"/>
  <chapter href="breads.dita"><topicmeta><navtitle>Breads of the world</navtitle></topicmeta></chapter>
  <appendix href="glossary.dita" navtitle="Glossary"/>
</bookmap>
""",
)

# 5. a DITA reference in Arabic with a properties table
w(
    "lumen_reference_ar.dita",
    """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE reference PUBLIC "-//OASIS//DTD DITA Reference//EN" "reference.dtd">
<reference id="ovens" xml:lang="ar">
  <title>أفران مخبز لومن</title>
  <refbody>
    <properties>
      <prophead><proptypehd>الفرن</proptypehd><propvaluehd>الحرارة</propvaluehd></prophead>
      <property><proptype>فرن الحجر</proptype><propvalue><ph>250</ph> درجة</propvalue></property>
      <property><proptype>فرن <b>البخار</b></proptype><propvalue><codeph>STEAM-2</codeph></propvalue></property>
    </properties>
    <section><title>السلامة</title><p>ارتد <term>القفازات</term> دائما.</p></section>
  </refbody>
</reference>
""",
)

# 6. a topic with one section per language (11 languages)
body = "\n".join(f'    <section id="s-{c}" xml:lang="{c}"><title>{c}</title><p>{t}</p></section>' for c, t in LANGS)
w(
    "lumen_languages.dita",
    f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE topic PUBLIC "-//OASIS//DTD DITA Topic//EN" "topic.dtd">
<topic id="languages" xml:lang="en">
  <title>Welcome in eleven languages</title>
  <body>
{body}
  </body>
</topic>
""",
)

# 7. DocBook 4.5: DOCTYPE, lang, code skipped, inline and nested elements
w(
    "lumen_manual45.dbk",
    """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE book PUBLIC "-//OASIS//DTD DocBook XML V4.5//EN" "http://www.oasis-open.org/docbook/xml/4.5/docbookx.dtd">
<book lang="en">
  <title>Lumen Oven Manual</title>
  <chapter id="start" xreflabel="Getting started">
    <title>Getting started</title>
    <para>Turn the oven on with <guibutton>Power</guibutton>, then read <xref linkend="safety"/>.<footnote><para>The light stays green.</para></footnote></para>
    <para>Set the heat in <filename>/etc/oven.conf</filename> or run <command>oven --heat 250</command>.</para>
    <programlisting language="sh">oven --heat 250 --steam on</programlisting>
    <remark>Check the numbers with the supplier.</remark>
    <itemizedlist><listitem><para>Preheat for <emphasis role="strong">40</emphasis> minutes.</para></listitem><listitem><para>Load the <productname>Lumen Stone</productname> tray.</para></listitem></itemizedlist>
  </chapter>
  <chapter id="safety">
    <title>Safety</title>
    <para>Press <keycombo><keycap>Ctrl</keycap><keycap>S</keycap></keycombo> to stop. <ulink url="http://example.invalid/help">Online help</ulink>.</para>
    <screen>$ oven --stop</screen>
  </chapter>
</book>
""",
)

# 8. DocBook 5: namespace, xml:lang, xreflabel attribute, CDATA
w(
    "lumen_article5.docbook",
    """<?xml version="1.0" encoding="UTF-8"?>
<article xmlns="http://docbook.org/ns/docbook" xmlns:xlink="http://www.w3.org/1999/xlink" version="5.0" xml:lang="en">
  <info><title>Lumen Bakery news</title><author><personname>Ana Baker</personname></author></info>
  <section xml:id="new" xreflabel="New breads">
    <title>New breads</title>
    <para>Try our <emphasis>rye</emphasis> loaf and the <link xlink:href="https://example.invalid/seeded">seeded</link> bar.<footnote><para>Contains sesame.</para></footnote></para>
    <para>Prices start at <literal>€4.50</literal> &amp; go down on <quote>Fresh Friday</quote>.</para>
    <para><![CDATA[Fish & chips <Friday>]]></para>
    <programlisting>price --list</programlisting>
  </section>
</article>
""",
)
