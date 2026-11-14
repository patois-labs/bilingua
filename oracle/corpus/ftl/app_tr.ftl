### Lumen Bakery (tr)

# Shown on the login button
login = Giriş yap
    .title = Giriş yapmak için tıklayın
    .accesskey = L

## Shop

-brand-name = Lumen Fırın
    .gender = feminine

welcome = { $name }, { -brand-name }: Her sabah taze ekmek

loaves = { $count ->
    [one] { $count } bir ekmek
   *[other] { $count } ekmek
}

tagline =
    Her sabah taze ekmek
    { -brand-name }

price = { NUMBER($amount, minimumFractionDigits: 2) }
