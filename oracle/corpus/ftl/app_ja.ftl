### Lumen Bakery (ja)

# Shown on the login button
login = ログイン
    .title = クリックしてログイン
    .accesskey = L

## Shop

-brand-name = ルーメンベーカリー
    .gender = feminine

welcome = { $name }, { -brand-name }: 毎朝焼きたてのパン

loaves = { $count ->
    [one] { $count } パン1個
   *[other] { $count } 個のパン
}

tagline =
    毎朝焼きたてのパン
    { -brand-name }

price = { NUMBER($amount, minimumFractionDigits: 2) }
