### Lumen Bakery (es)

# Shown on the login button
login = Iniciar sesión
    .title = Pulse para iniciar sesión
    .accesskey = L

## Shop

-brand-name = Panadería Lumen
    .gender = feminine

welcome = { $name }, { -brand-name }: Pan fresco cada mañana

loaves = { $count ->
    [one] { $count } un pan
   *[other] { $count } panes
}

tagline =
    Pan fresco cada mañana
    { -brand-name }

price = { NUMBER($amount, minimumFractionDigits: 2) }
