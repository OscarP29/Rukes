// @rukes.name        Ordenar ventana
// @rukes.description Mueve la ventana indicada a la mitad izquierda de la pantalla
// @rukes.alias       izquierda
// @rukes.arg         ventana:text:required
// @rukes.output      silent
//
// Ejemplo de la API de ventanas de Rhai: windows(), focus_window(), move_window().

let w = args.ventana;
if !move_window(w, 0, 0, 960, 1040) {
    throw `No hay ninguna ventana que contenga «${w}»`;
}
focus_window(w);
