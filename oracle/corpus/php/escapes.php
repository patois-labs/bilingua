<?php
return [
    'sq' => 'It\'s a C:\\path value and \n stays literal',
    'dq' => "Tab\there \x41\101 \u{e9} \$5 \q and \"quotes\"\n",
    'concat' => 'First half, ' . "second half" . ' third',
    'keys' => [-1 => 'Minus one', 7 => 'Seven', 'Eight', '12' => 'Twelve', 'Thirteen'],
    'empty' => '',
    'symbols' => '...',
    'num' => 3.14,
];
