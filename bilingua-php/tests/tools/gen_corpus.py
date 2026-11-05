"""Writes the oracle corpus oracle/corpus/php/ (run: python gen_corpus.py ../../../oracle/corpus/php)."""
import os
import sys

out = sys.argv[1]
os.makedirs(out, exist_ok=True)

LANGS = {
    "en": ("These credentials do not match our records.", "Too many login attempts. Please try again in :seconds seconds.",
           "Welcome, :name! You have <strong>:count</strong> new messages.", "{0} No apples|[1,19] Some apples|[20,*] Many apples", "Save changes", "Name: %1$s (%2$d)"),
    "de": ("Diese Zugangsdaten stimmen nicht mit unseren Aufzeichnungen \u00fcberein.", "Zu viele Anmeldeversuche. Bitte in :seconds Sekunden erneut versuchen.",
           "Willkommen, :name! Sie haben <strong>:count</strong> neue Nachrichten.", "{0} Keine \u00c4pfel|[1,19] Einige \u00c4pfel|[20,*] Viele \u00c4pfel", "\u00c4nderungen speichern", "Name: %1$s (%2$d)"),
    "fr": ("Ces identifiants ne correspondent pas \u00e0 nos enregistrements.", "Trop de tentatives de connexion. R\u00e9essayez dans :seconds secondes.",
           "Bienvenue, :name ! Vous avez <strong>:count</strong> nouveaux messages.", "{0} Aucune pomme|[1,19] Quelques pommes|[20,*] Beaucoup de pommes", "Enregistrer les modifications", "Nom : %1$s (%2$d)"),
    "es": ("Estas credenciales no coinciden con nuestros registros.", "Demasiados intentos de acceso. Int\u00e9ntelo de nuevo en :seconds segundos.",
           "\u00a1Bienvenido, :name! Tiene <strong>:count</strong> mensajes nuevos.", "{0} Ninguna manzana|[1,19] Algunas manzanas|[20,*] Muchas manzanas", "Guardar cambios", "Nombre: %1$s (%2$d)"),
    "pt": ("Essas credenciais n\u00e3o correspondem aos nossos registros.", "Muitas tentativas de login. Tente novamente em :seconds segundos.",
           "Bem-vindo, :name! Voc\u00ea tem <strong>:count</strong> novas mensagens.", "{0} Nenhuma ma\u00e7\u00e3|[1,19] Algumas ma\u00e7\u00e3s|[20,*] Muitas ma\u00e7\u00e3s", "Salvar altera\u00e7\u00f5es", "Nome: %1$s (%2$d)"),
    "ru": ("\u042d\u0442\u0438 \u0443\u0447\u0451\u0442\u043d\u044b\u0435 \u0434\u0430\u043d\u043d\u044b\u0435 \u043d\u0435 \u0441\u043e\u0432\u043f\u0430\u0434\u0430\u044e\u0442 \u0441 \u043d\u0430\u0448\u0438\u043c\u0438 \u0437\u0430\u043f\u0438\u0441\u044f\u043c\u0438.",
           "\u0421\u043b\u0438\u0448\u043a\u043e\u043c \u043c\u043d\u043e\u0433\u043e \u043f\u043e\u043f\u044b\u0442\u043e\u043a \u0432\u0445\u043e\u0434\u0430. \u041f\u043e\u043f\u0440\u043e\u0431\u0443\u0439\u0442\u0435 \u0441\u043d\u043e\u0432\u0430 \u0447\u0435\u0440\u0435\u0437 :seconds \u0441\u0435\u043a\u0443\u043d\u0434.",
           "\u0414\u043e\u0431\u0440\u043e \u043f\u043e\u0436\u0430\u043b\u043e\u0432\u0430\u0442\u044c, :name! \u0423 \u0432\u0430\u0441 <strong>:count</strong> \u043d\u043e\u0432\u044b\u0445 \u0441\u043e\u043e\u0431\u0449\u0435\u043d\u0438\u0439.",
           "{0} \u041d\u0435\u0442 \u044f\u0431\u043b\u043e\u043a|[1,4] \u041d\u0435\u0441\u043a\u043e\u043b\u044c\u043a\u043e \u044f\u0431\u043b\u043e\u043a|[5,*] \u041c\u043d\u043e\u0433\u043e \u044f\u0431\u043b\u043e\u043a", "\u0421\u043e\u0445\u0440\u0430\u043d\u0438\u0442\u044c \u0438\u0437\u043c\u0435\u043d\u0435\u043d\u0438\u044f", "\u0418\u043c\u044f: %1$s (%2$d)"),
    "tr": ("Bu kimlik bilgileri kay\u0131tlar\u0131m\u0131zla e\u015fle\u015fmiyor.", "\u00c7ok fazla giri\u015f denemesi. L\u00fctfen :seconds saniye sonra tekrar deneyin.",
           "Ho\u015f geldiniz, :name! <strong>:count</strong> yeni iletiniz var.", "{0} Elma yok|[1,19] Birka\u00e7 elma|[20,*] \u00c7ok say\u0131da elma", "De\u011fi\u015fiklikleri kaydet", "Ad: %1$s (%2$d)"),
    "ar": ("\u0628\u064a\u0627\u0646\u0627\u062a \u0627\u0644\u0627\u0639\u062a\u0645\u0627\u062f \u0647\u0630\u0647 \u0644\u0627 \u062a\u062a\u0637\u0627\u0628\u0642 \u0645\u0639 \u0633\u062c\u0644\u0627\u062a\u0646\u0627.",
           "\u0645\u062d\u0627\u0648\u0644\u0627\u062a \u062a\u0633\u062c\u064a\u0644 \u062f\u062e\u0648\u0644 \u0643\u062b\u064a\u0631\u0629 \u062c\u062f\u0627\u064b. \u0627\u0644\u0645\u0631\u062c\u0648 \u0627\u0644\u0645\u062d\u0627\u0648\u0644\u0629 \u0645\u0631\u0629 \u0623\u062e\u0631\u0649 \u0628\u0639\u062f :seconds \u062b\u0627\u0646\u064a\u0629.",
           "\u0645\u0631\u062d\u0628\u0627\u064b \u064a\u0627 :name! \u0644\u062f\u064a\u0643 <strong>:count</strong> \u0631\u0633\u0627\u0626\u0644 \u062c\u062f\u064a\u062f\u0629.",
           "{0} \u0644\u0627 \u062a\u0641\u0627\u062d|[1,2] \u062a\u0641\u0627\u062d\u062a\u0627\u0646|[3,*] \u0627\u0644\u0643\u062b\u064a\u0631 \u0645\u0646 \u0627\u0644\u062a\u0641\u0627\u062d", "\u062d\u0641\u0638 \u0627\u0644\u062a\u063a\u064a\u064a\u0631\u0627\u062a", "\u0627\u0644\u0627\u0633\u0645: %1$s (%2$d)"),
    "ja": ("\u3053\u306e\u8cc7\u683c\u60c5\u5831\u306f\u8a18\u9332\u3068\u4e00\u81f4\u3057\u307e\u305b\u3093\u3002", "\u30ed\u30b0\u30a4\u30f3\u306e\u8a66\u884c\u56de\u6570\u304c\u591a\u3059\u304e\u307e\u3059\u3002:seconds\u79d2\u5f8c\u306b\u3082\u3046\u4e00\u5ea6\u304a\u8a66\u3057\u304f\u3060\u3055\u3044\u3002",
           "\u3088\u3046\u3053\u305d\u3001:name\u3055\u3093\uff01<strong>:count</strong>\u4ef6\u306e\u65b0\u3057\u3044\u30e1\u30c3\u30bb\u30fc\u30b8\u304c\u3042\u308a\u307e\u3059\u3002", "{0} \u308a\u3093\u3054\u306f\u3042\u308a\u307e\u305b\u3093|[1,19] \u308a\u3093\u3054\u304c\u3044\u304f\u3064\u304b|[20,*] \u308a\u3093\u3054\u304c\u305f\u304f\u3055\u3093", "\u5909\u66f4\u3092\u4fdd\u5b58", "\u540d\u524d: %1$s (%2$d)"),
    "zh": ("\u8fd9\u4e9b\u51ed\u636e\u4e0e\u6211\u4eec\u7684\u8bb0\u5f55\u4e0d\u5339\u914d\u3002", "\u767b\u5f55\u5c1d\u8bd5\u6b21\u6570\u8fc7\u591a\u3002\u8bf7\u5728 :seconds \u79d2\u540e\u91cd\u8bd5\u3002",
           "\u6b22\u8fce\uff0c:name\uff01\u60a8\u6709 <strong>:count</strong> \u6761\u65b0\u6d88\u606f\u3002", "{0} \u6ca1\u6709\u82f9\u679c|[1,19] \u4e00\u4e9b\u82f9\u679c|[20,*] \u5f88\u591a\u82f9\u679c", "\u4fdd\u5b58\u66f4\u6539", "\u59d3\u540d\uff1a%1$s (%2$d)"),
    "ko": ("\uc774 \uc790\uaca9 \uc99d\uba85\uc740 \uae30\ub85d\uacfc \uc77c\uce58\ud558\uc9c0 \uc54a\uc2b5\ub2c8\ub2e4.", "\ub85c\uadf8\uc778 \uc2dc\ub3c4\uac00 \ub108\ubb34 \ub9ce\uc2b5\ub2c8\ub2e4. :seconds\ucd08 \ud6c4\uc5d0 \ub2e4\uc2dc \uc2dc\ub3c4\ud574 \uc8fc\uc138\uc694.",
           "\ud658\uc601\ud569\ub2c8\ub2e4, :name\ub2d8! \uc0c8 \uba54\uc2dc\uc9c0\uac00 <strong>:count</strong>\uac1c \uc788\uc2b5\ub2c8\ub2e4.", "{0} \uc0ac\uacfc \uc5c6\uc74c|[1,19] \uc0ac\uacfc \uba87 \uac1c|[20,*] \uc0ac\uacfc \ub9ce\uc774", "\ubcc0\uacbd \uc0ac\ud56d \uc800\uc7a5", "\uc774\ub984: %1$s (%2$d)"),
}


def laravel(t, nl="\n"):
    failed, throttle, welcome, apples, save, name = t
    lines = [
        "<?php", "",
        "declare(strict_types=1);", "",
        "// Authentication language lines",
        "return [",
        "    /*",
        "     | Used during authentication.",
        "     */",
        "    'failed' => '%s'," % failed.replace("\\", "\\\\").replace("'", "\\'"),
        "    'throttle' => \"%s\"," % throttle.replace("\\", "\\\\").replace('"', '\\"').replace("$", "\\$"),
        "    'welcome' => '%s'," % welcome.replace("'", "\\'"),
        "    'apples' => '%s'," % apples,
        "    'buttons' => [",
        "        'save' => '%s'," % save,
        "        'close' => 'x',",
        "        'sep' => '---',",
        "    ],",
        "    'label' => \"%s\"," % name.replace("$", "\\$"),
        "    'legacy' => array('ok' => '%s', 5 => 'Five')," % save,
        "    'count' => 42,",
        "    'flag' => true,",
        "];",
        "",
    ]
    return nl.join(lines)


def w(name, data):
    if isinstance(data, str):
        data = data.encode("utf-8")
    with open(os.path.join(out, name), "wb") as fh:
        fh.write(data)


for code, t in LANGS.items():
    w(f"lang_{code}.php", laravel(t))

w("lang_en_utf8bom_crlf.php", b"\xef\xbb\xbf" + laravel(LANGS["en"], "\r\n").encode("utf-8"))
w("lang_fr_cp1252.php", laravel(LANGS["fr"]).encode("cp1252"))
w("lang_de_utf16le.php", b"\xff\xfe" + laravel(LANGS["de"]).encode("utf-16-le"))
w("lang_ja_utf16be.php", b"\xfe\xff" + laravel(LANGS["ja"]).encode("utf-16-be"))

w("ci3_email_lang.php", r"""<?php
defined('BASEPATH') OR exit('No direct script access allowed');

$lang['email_must_be_array'] = 'The email validation method must be passed an array.';
$lang['email_invalid_address'] = 'Invalid email address: %s';
$lang['email_attachment_missing'] = 'Unable to locate the following email attachment: %s';
$lang['form']['label'] = "Name: %1\$s (%2\$d)";
$lang['form']['hint'] = 'Click <a href="/help">here</a> for help.';
$lang['db_unable_to_connect'] = 'Unable to connect to your database server.';
?>
""")
w("heredoc_nowdoc.php", r"""<?php
return [
    'h' => <<<EOT
        Line one
          Line two \u{263A}
        EOT,
    'n' => <<<'TXT'
Raw $not {$interp} \n
TXT,
    'terms' => <<<HTML
<p>Welcome, :name.</p>
HTML
,
    'after' => 'Still a normal string',
];
""")
w("escapes.php", r"""<?php
return [
    'sq' => 'It\'s a C:\\path value and \n stays literal',
    'dq' => "Tab\there \x41\101 \u{e9} \$5 \q and \"quotes\"\n",
    'concat' => 'First half, ' . "second half" . ' third',
    'keys' => [-1 => 'Minus one', 7 => 'Seven', 'Eight', '12' => 'Twelve', 'Thirteen'],
    'empty' => '',
    'symbols' => '...',
    'num' => 3.14,
];
""")
w("comments_old_array.php", """<?php
# hash comment
/* block */ return array( // inline
    'a' => array(
        'b' => 'Nested old-style value', // trailing
        'c' => 'Second :value ? done',
    ),
    // 'commented' => 'Not a unit',
    'plain' => 'Plain value'
);
""")
w("closed_tag.php", "<?php\nreturn ['one' => 'First value', 'two' => 'Second value'];\n?>\n")
print("wrote", len(os.listdir(out)), "files")
