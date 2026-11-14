<?php
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
