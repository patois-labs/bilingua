### Lumen Bakery (pt)

# Shown on the login button
login = Entrar
    .title = Clique para entrar
    .accesskey = L

## Shop

-brand-name = Padaria Lumen
    .gender = feminine

welcome = { $name }, { -brand-name }: Pão fresco toda manhã

loaves = { $count ->
    [one] { $count } um pão
   *[other] { $count } pães
}

tagline =
    Pão fresco toda manhã
    { -brand-name }

price = { NUMBER($amount, minimumFractionDigits: 2) }
