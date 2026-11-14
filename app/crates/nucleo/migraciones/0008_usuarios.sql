-- Usuarios, roles y bitácora de accesos (ESPECIFICACION_SUA.md, §11).
--
-- Mientras la tabla `usuario` esté vacía el programa no pide inicio de sesión.
-- La contraseña no se guarda: solo su huella Argon2 (con sal propia de cada
-- usuario). Un usuario no se borra, se desactiva: su nombre queda en la bitácora.
CREATE TABLE usuario (
    id              INTEGER PRIMARY KEY,
    nombre          TEXT NOT NULL UNIQUE CHECK (length(nombre) BETWEEN 3 AND 20),
    nombre_completo TEXT NOT NULL DEFAULT '',
    rol             TEXT NOT NULL CHECK (rol IN ('administrador', 'capturista', 'consulta')),
    huella          TEXT NOT NULL,
    activo          INTEGER NOT NULL DEFAULT 1 CHECK (activo IN (0, 1)),
    creado          TEXT NOT NULL
) STRICT;

-- Quién hizo qué y cuándo (hora UTC). `detalle` identifica el registro tocado
-- (registro patronal, NSS, periodo), sin nombres ni importes.
CREATE TABLE bitacora_accesos (
    id      INTEGER PRIMARY KEY,
    momento TEXT NOT NULL,
    usuario TEXT NOT NULL,
    accion  TEXT NOT NULL,
    detalle TEXT NOT NULL DEFAULT ''
) STRICT;
