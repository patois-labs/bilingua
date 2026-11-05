### Lumen Bakery (zh)

# Shown on the login button
login = 登录
    .title = 点击登录
    .accesskey = L

## Shop

-brand-name = 流明面包店
    .gender = feminine

welcome = { $name }, { -brand-name }: 每天早晨新鲜面包

loaves = { $count ->
    [one] { $count } 一个面包
   *[other] { $count } 个面包
}

tagline =
    每天早晨新鲜面包
    { -brand-name }

price = { NUMBER($amount, minimumFractionDigits: 2) }
