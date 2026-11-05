# Plugin «{{name}}» de Rukes (protocolo: docs/plugin-protocol.md).
#
# Rukes lo arranca la primera vez que se usa y le habla por stdin/stdout con JSON, un mensaje
# por línea. Lo que escribas con Write-Error / [Console]::Error va al log de Rukes. Al guardar
# este archivo o plugin.toml, Rukes lo recarga solo.

$ErrorActionPreference = "Stop"
$utf8 = New-Object System.Text.UTF8Encoding $false
[Console]::InputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$stdin = New-Object System.IO.StreamReader ([Console]::OpenStandardInput()), $utf8
$stdout = New-Object System.IO.StreamWriter ([Console]::OpenStandardOutput()), $utf8
$stdout.AutoFlush = $true
$config = @{}

function Send($msg) {
    $stdout.WriteLine(($msg | ConvertTo-Json -Compress -Depth 10))
}

# Resultados para lo escrito tras la palabra clave.
function Query($params) {
    $text = [string]$params.text
    if ($text -eq "") {
        return , @(@{ title = "Escribe algo para probar «{{name}}»"; actions = @() })
    }
    return , @(@{
        id       = "mayusculas"
        title    = $text.ToUpper()
        subtitle = "Enter copia el texto en mayúsculas"
        data     = $text.ToUpper()
    })
}

# Lo que pasa al pulsar Enter: copy, open, notify, query, results, hide o keep_open.
function Action($params) {
    return @{ copy = $params.item.data }
}

while ($null -ne ($line = $stdin.ReadLine())) {
    $msg = $line | ConvertFrom-Json
    if ($null -eq $msg.id) { continue }  # notificación «cancel»: no se responde
    try {
        switch ($msg.method) {
            "initialize" { $config = $msg.params.config; $result = @{} }
            "query" { $result = Query $msg.params }
            default { $result = Action $msg.params }
        }
        Send @{ id = $msg.id; result = $result }
    } catch {
        Send @{ id = $msg.id; error = @{ message = "$_" } }
    }
}
