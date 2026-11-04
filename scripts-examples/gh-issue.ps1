# @rukes.name        Crear issue en GitHub
# @rukes.description Abre un issue nuevo en el repo indicado (necesita GitHub CLI, «gh»)
# @rukes.icon        github
# @rukes.alias       gh issue
# @rukes.arg         title:text:required
# @rukes.output      notification
# @rukes.timeout     15s
# @rukes.cwd         ~/Desktop/Proyectos/mi-repo
#
# Uso: «gh issue Mi bug». Cambia @rukes.cwd por la carpeta de tu repositorio.

param([Parameter(Mandatory)][string]$title)

$url = gh issue create --title $title --body "Creado desde Rukes" 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Error $url
    exit $LASTEXITCODE
}
Write-Output "Issue creado: $url"
