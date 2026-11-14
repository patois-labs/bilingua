### Lumen Bakery (de)

# Shown on the login button
login = Anmelden
    .title = Zum Anmelden klicken
    .accesskey = L

## Shop

-brand-name = Lumen Backstube
    .gender = feminine

welcome = { $name }, { -brand-name }: Jeden Morgen frisches Brot

loaves = { $count ->
    [one] { $count } ein Brot
   *[other] { $count } Brote
}

tagline =
    Jeden Morgen frisches Brot
    { -brand-name }

price = { NUMBER($amount, minimumFractionDigits: 2) }
