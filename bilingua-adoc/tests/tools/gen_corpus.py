"""Writes the made-up AsciiDoc corpus into oracle/corpus/adoc (run once; the files are committed).

    python gen_corpus.py ../../../oracle/corpus/adoc
"""

import os
import sys

OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)

TEMPLATE = """= {title}
Jane Doe <jane@example.com>
v1.2, 2026-10-01
:product: Lumen Bakery
:toc:
:description: {desc}
:keywords: {kw}

// A comment that stays.

{intro} *{bold}* {with} {{product}} `code` {see} https://example.com[{site}].
{second} _{italic}_ {and} kbd:[Ctrl+S].

== {sec} ==

NOTE: {note}

.{btitle}
* {item1}
* {item2} <<anchor,{xref}>>
** {nested}
* [x] {done}
. {step1}
. {step2} +
{cont}

{term}:: {def}

[source,python]
----
print("not translated")  # {code}
----

[cols="1,2"]
|===
|{h1} |{h2}

|{c1}
|{c2} \\| {c3}
|===

include::partials/legal.adoc[]

{last} footnote:[{fn}]
"""

LANGS = {
    "en": dict(title="Order Guide", desc="How to order bread online.", kw="bread, order", intro="Welcome to our", bold="bakery", with_="with", see="and see", site="our site", second="Fresh bread is", italic="baked daily", and_="and", sec="Getting Started", note="Read the guide before you order.", btitle="Pickup options", item1="Pick up in store", item2="Delivery, see", xref="the map", nested="Free over 20 dollars", done="Account created", step1="Choose a loaf", step2="Pay online", cont="Then wait for the message.", term="Sourdough", def_="A slow-fermented loaf.", code="a comment", h1="Item", h2="Description", c1="Rye", c2="Dark bread", c3="heavy", last="Thank you for ordering.", fn="Prices include tax."),
    "fr": dict(title="Guide de commande", desc="Comment commander du pain en ligne.", kw="pain, commande", intro="Bienvenue dans notre", bold="boulangerie", with_="avec", see="et voir", site="notre site", second="Le pain frais est", italic="cuit chaque jour", and_="et", sec="Pour commencer", note="Lisez le guide avant de commander.", btitle="Options de retrait", item1="Retrait en magasin", item2="Livraison, voir", xref="la carte", nested="Gratuite dès 20 euros", done="Compte créé", step1="Choisissez un pain", step2="Payez en ligne", cont="Puis attendez le message.", term="Levain", def_="Un pain à fermentation lente.", code="un commentaire", h1="Article", h2="Description", c1="Seigle", c2="Pain foncé", c3="lourd", last="Merci de votre commande.", fn="Les prix incluent la taxe."),
    "ja": dict(title="注文ガイド", desc="パンをオンラインで注文する方法。", kw="パン, 注文", intro="ようこそ", bold="ベーカリー", with_="へ", see="詳しくは", site="サイト", second="新鮮なパンは", italic="毎日焼きたて", and_="です", sec="はじめに", note="ご注文の前にガイドをお読みください。", btitle="受け取り方法", item1="店頭で受け取り", item2="配達、参照", xref="地図", nested="20ドル以上で無料", done="アカウント作成済み", step1="パンを選ぶ", step2="オンラインで支払う", cont="その後メッセージをお待ちください。", term="サワードウ", def_="ゆっくり発酵させたパン。", code="コメント", h1="品名", h2="説明", c1="ライ麦", c2="黒パン", c3="重い", last="ご注文ありがとうございます。", fn="価格は税込です。"),
    "ar": dict(title="دليل الطلب", desc="كيفية طلب الخبز عبر الإنترنت.", kw="خبز, طلب", intro="مرحبا بكم في", bold="مخبزنا", with_="مع", see="وانظر", site="موقعنا", second="الخبز الطازج", italic="يخبز يوميا", and_="و", sec="البدء", note="اقرأ الدليل قبل الطلب.", btitle="خيارات الاستلام", item1="الاستلام من المتجر", item2="التوصيل، انظر", xref="الخريطة", nested="مجاني فوق 20 دولارا", done="تم إنشاء الحساب", step1="اختر رغيفا", step2="ادفع عبر الإنترنت", cont="ثم انتظر الرسالة.", term="العجين المخمر", def_="رغيف مخمر ببطء.", code="تعليق", h1="الصنف", h2="الوصف", c1="الجاودار", c2="خبز داكن", c3="ثقيل", last="شكرا لطلبك.", fn="الأسعار تشمل الضريبة."),
    "zh": dict(title="订购指南", desc="如何在线订购面包。", kw="面包, 订购", intro="欢迎来到我们的", bold="面包店", with_="使用", see="并查看", site="我们的网站", second="新鲜面包", italic="每天烘焙", and_="并且", sec="开始", note="订购前请阅读指南。", btitle="取货方式", item1="到店取货", item2="送货，见", xref="地图", nested="满20美元免费", done="账户已创建", step1="选择面包", step2="在线支付", cont="然后等待消息。", term="酸面团", def_="慢发酵的面包。", code="注释", h1="品名", h2="说明", c1="黑麦", c2="黑面包", c3="厚重", last="感谢您的订购。", fn="价格含税。"),
    "ko": dict(title="주문 안내", desc="온라인으로 빵을 주문하는 방법.", kw="빵, 주문", intro="저희", bold="베이커리", with_="에", see="오신 것을 환영합니다", site="사이트", second="신선한 빵은", italic="매일 구워집니다", and_="그리고", sec="시작하기", note="주문 전에 안내를 읽어 주세요.", btitle="수령 방법", item1="매장 수령", item2="배달, 참조", xref="지도", nested="20달러 이상 무료", done="계정 생성됨", step1="빵 고르기", step2="온라인 결제", cont="그런 다음 메시지를 기다리세요.", term="사워도우", def_="천천히 발효한 빵.", code="주석", h1="품목", h2="설명", c1="호밀", c2="검은 빵", c3="묵직함", last="주문해 주셔서 감사합니다.", fn="가격에는 세금이 포함됩니다."),
    "ru": dict(title="Как заказать", desc="Как заказать хлеб онлайн.", kw="хлеб, заказ", intro="Добро пожаловать в нашу", bold="пекарню", with_="с", see="и смотрите", site="наш сайт", second="Свежий хлеб", italic="печётся ежедневно", and_="и", sec="Начало работы", note="Прочитайте руководство перед заказом.", btitle="Способы получения", item1="Самовывоз", item2="Доставка, см.", xref="карту", nested="Бесплатно от 20 долларов", done="Аккаунт создан", step1="Выберите буханку", step2="Оплатите онлайн", cont="Затем ждите сообщения.", term="Закваска", def_="Хлеб медленного брожения.", code="комментарий", h1="Товар", h2="Описание", c1="Рожь", c2="Тёмный хлеб", c3="тяжёлый", last="Спасибо за заказ.", fn="Цены включают налог."),
    "de": dict(title="Bestellanleitung", desc="So bestellen Sie Brot online.", kw="Brot, Bestellung", intro="Willkommen in unserer", bold="Bäckerei", with_="mit", see="und siehe", site="unsere Seite", second="Frisches Brot wird", italic="täglich gebacken", and_="und", sec="Erste Schritte", note="Lesen Sie die Anleitung vor der Bestellung.", btitle="Abholoptionen", item1="Abholung im Laden", item2="Lieferung, siehe", xref="die Karte", nested="Kostenlos ab 20 Euro", done="Konto erstellt", step1="Brot wählen", step2="Online bezahlen", cont="Dann auf die Nachricht warten.", term="Sauerteig", def_="Ein langsam gegorenes Brot.", code="ein Kommentar", h1="Artikel", h2="Beschreibung", c1="Roggen", c2="Dunkles Brot", c3="schwer", last="Danke für Ihre Bestellung.", fn="Preise inklusive Steuer."),
}

for name, d in LANGS.items():
    d = dict(d)
    d["with"] = d.pop("with_")
    d["and"] = d.pop("and_")
    d["def"] = d.pop("def_")
    text = TEMPLATE.format(**d)
    with open(os.path.join(OUT, f"orders_{name}.adoc"), "wb") as f:
        f.write(text.encode("utf-8"))

fr = LANGS["fr"]
fr = dict(fr)
fr["with"] = fr.pop("with_")
fr["and"] = fr.pop("and_")
fr["def"] = fr.pop("def_")
fr_text = TEMPLATE.format(**fr)
open(os.path.join(OUT, "orders_fr_cp1252.adoc"), "wb").write(fr_text.encode("cp1252"))
open(os.path.join(OUT, "orders_fr_bom_crlf.adoc"), "wb").write(b"\xef\xbb\xbf" + fr_text.replace("\n", "\r\n").encode("utf-8"))
open(os.path.join(OUT, "orders_fr_utf16.adoc"), "wb").write(b"\xff\xfe" + fr_text.encode("utf-16-le"))

open(os.path.join(OUT, "minimal.adoc"), "wb").write(b"Hello world.\n")
open(os.path.join(OUT, "tables_and_blocks.asciidoc"), "wb").write(
    b"""= Blocks
:author: X

[NOTE]
====
An admonition block body is translated.
====

****
A sidebar with *bold* text.
****

====
An example block.

A second paragraph.
====

[quote, Someone]
____
A quotation here.
____

....
literal never translated
....

----
listing never translated
----

|===
|Name |Value
a|An *AsciiDoc* cell
|===
.. Step one
.. Step two
"""
)
