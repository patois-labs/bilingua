### Lumen Bakery (ar)

# Shown on the login button
login = تسجيل الدخول
    .title = انقر لتسجيل الدخول
    .accesskey = L

## Shop

-brand-name = مخبز لومن
    .gender = feminine

welcome = { $name }, { -brand-name }: خبز طازج كل صباح

loaves = { $count ->
    [one] { $count } رغيف واحد
   *[other] { $count } أرغفة
}

tagline =
    خبز طازج كل صباح
    { -brand-name }

price = { NUMBER($amount, minimumFractionDigits: 2) }
