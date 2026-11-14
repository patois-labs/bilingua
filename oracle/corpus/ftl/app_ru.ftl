### Lumen Bakery (ru)

# Shown on the login button
login = Войти
    .title = Нажмите, чтобы войти
    .accesskey = L

## Shop

-brand-name = Пекарня Люмен
    .gender = feminine

welcome = { $name }, { -brand-name }: Свежий хлеб каждое утро

loaves = { $count ->
    [one] { $count } один хлеб
   *[other] { $count } хлебов
}

tagline =
    Свежий хлеб каждое утро
    { -brand-name }

price = { NUMBER($amount, minimumFractionDigits: 2) }
