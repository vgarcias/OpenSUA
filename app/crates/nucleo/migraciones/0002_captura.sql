-- Tablas de captura. Nombres de campo del SUA.MDB original en minúsculas
-- (Esquema_SUA_MDB.md, sección 2). Importes en centavos (INTEGER), fechas
-- como texto ISO AAAA-MM-DD, claves como texto de longitud validada.

CREATE TABLE patron (
    reg_pat   TEXT PRIMARY KEY CHECK (length(reg_pat) = 11),
    rfc_pat   TEXT NOT NULL CHECK (length(rfc_pat) IN (10, 12, 13)),
    nom_pat   TEXT NOT NULL CHECK (length(nom_pat) <= 50),
    act_pat   TEXT NOT NULL DEFAULT '' CHECK (length(act_pat) <= 40),
    dom_pat   TEXT NOT NULL DEFAULT '' CHECK (length(dom_pat) <= 50),
    mun_pat   TEXT NOT NULL DEFAULT '' CHECK (length(mun_pat) <= 50),
    cpp_pat   TEXT NOT NULL DEFAULT '' CHECK (length(cpp_pat) IN (0, 5)),
    ent_pat   TEXT NOT NULL DEFAULT '' CHECK (length(ent_pat) <= 2),
    tel_pat   TEXT NOT NULL DEFAULT '' CHECK (length(tel_pat) <= 15),
    rem_pat   TEXT NOT NULL DEFAULT '',
    zon_pat   TEXT NOT NULL DEFAULT '' CHECK (length(zon_pat) <= 1),
    del_pat   TEXT NOT NULL DEFAULT '' CHECK (length(del_pat) <= 4),
    tip_con   TEXT NOT NULL DEFAULT '',
    con_ven   TEXT NOT NULL DEFAULT '',
    ini_afil  TEXT NOT NULL DEFAULT '',
    pat_rep   TEXT NOT NULL DEFAULT '' CHECK (length(pat_rep) <= 50),
    clase     TEXT NOT NULL DEFAULT '',
    fraccion  TEXT NOT NULL DEFAULT '',
    styps     TEXT NOT NULL DEFAULT ''
) STRICT;

-- La prima de riesgos de trabajo cambia por vigencia; no se sobrescribe.
-- 7.58875 % se guarda como 758875 (cienmilésimas de punto porcentual).
CREATE TABLE patron_prima_rt (
    reg_pat             TEXT NOT NULL REFERENCES patron(reg_pat) ON UPDATE CASCADE ON DELETE RESTRICT,
    vigente_desde       TEXT NOT NULL CHECK (length(vigente_desde) = 7),
    prima_cienmilesimas INTEGER NOT NULL CHECK (prima_cienmilesimas BETWEEN 0 AND 9999999),
    PRIMARY KEY (reg_pat, vigente_desde)
) STRICT;

CREATE TABLE asegura (
    reg_patr   TEXT NOT NULL REFERENCES patron(reg_pat) ON UPDATE CASCADE ON DELETE RESTRICT,
    num_afil   TEXT NOT NULL CHECK (length(num_afil) = 11),
    curp       TEXT NOT NULL DEFAULT '' CHECK (length(curp) IN (0, 18)),
    rfc_curp   TEXT NOT NULL DEFAULT '' CHECK (length(rfc_curp) IN (0, 10, 13)),
    nom_aseg   TEXT NOT NULL CHECK (length(nom_aseg) <= 50),
    sal_imss   INTEGER NOT NULL DEFAULT 0 CHECK (sal_imss >= 0),
    sal_info   INTEGER NOT NULL DEFAULT 0 CHECK (sal_info >= 0),
    fec_alt    TEXT NOT NULL DEFAULT '',
    fec_baj    TEXT NOT NULL DEFAULT '',
    tip_tra    TEXT NOT NULL DEFAULT '' CHECK (length(tip_tra) <= 1),
    sem_jord   TEXT NOT NULL DEFAULT '' CHECK (length(sem_jord) <= 1),
    pag_info   TEXT NOT NULL DEFAULT '' CHECK (length(pag_info) IN (0, 10)),
    tip_dsc    TEXT NOT NULL DEFAULT '' CHECK (length(tip_dsc) <= 1),
    val_dsc    INTEGER NOT NULL DEFAULT 0,
    cve_ubc    TEXT NOT NULL DEFAULT '' CHECK (length(cve_ubc) <= 17),
    fec_dsc    TEXT NOT NULL DEFAULT '',
    fec_findsc TEXT NOT NULL DEFAULT '',
    arti_33    TEXT NOT NULL DEFAULT '',
    sal_ar33   INTEGER NOT NULL DEFAULT 0,
    tra_peniv  TEXT NOT NULL DEFAULT '',
    estado     TEXT NOT NULL DEFAULT '',
    cve_mun    TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (reg_patr, num_afil)
) STRICT;

CREATE TABLE afiliacion (
    reg_patr  TEXT NOT NULL,
    num_afil  TEXT NOT NULL,
    cpp_trab  TEXT NOT NULL DEFAULT '',
    fec_nac   TEXT NOT NULL DEFAULT '',
    lug_nac   TEXT NOT NULL DEFAULT '',
    ent_trab  TEXT NOT NULL DEFAULT '',
    umf_trab  TEXT NOT NULL DEFAULT '',
    ocupa     TEXT NOT NULL DEFAULT '',
    sexo      TEXT NOT NULL DEFAULT '',
    tip_sal   TEXT NOT NULL DEFAULT '',
    jor_hor   TEXT NOT NULL DEFAULT '',
    cve_mun   TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (reg_patr, num_afil),
    FOREIGN KEY (reg_patr, num_afil) REFERENCES asegura(reg_patr, num_afil) ON UPDATE CASCADE ON DELETE CASCADE
) STRICT;

CREATE TABLE movtos (
    id        INTEGER PRIMARY KEY,
    reg_patr  TEXT NOT NULL,
    num_afil  TEXT NOT NULL,
    tip_movs  TEXT NOT NULL CHECK (length(tip_movs) = 2),
    fec_inic  TEXT NOT NULL,
    con_sec   INTEGER NOT NULL DEFAULT 0,
    num_dias  INTEGER NOT NULL DEFAULT 0 CHECK (num_dias >= 0),
    sal_movt  INTEGER NOT NULL DEFAULT 0 CHECK (sal_movt >= 0),
    sal_movt2 INTEGER NOT NULL DEFAULT 0,
    sal_movt3 INTEGER NOT NULL DEFAULT 0,
    mes_ano   TEXT NOT NULL DEFAULT '',
    fec_real  TEXT NOT NULL DEFAULT '',
    fol_inc   TEXT NOT NULL DEFAULT '',
    cve_movs  TEXT NOT NULL DEFAULT '',
    tip_inc   TEXT NOT NULL DEFAULT '',
    edo_mov   TEXT NOT NULL DEFAULT '0' CHECK (edo_mov IN ('0', '1', '2', '3', '4')),
    fec_ext   TEXT NOT NULL DEFAULT '',
    art_33    TEXT NOT NULL DEFAULT '',
    tip_sal   TEXT NOT NULL DEFAULT '',
    tip_rie   TEXT NOT NULL DEFAULT '',
    tip_rec   TEXT NOT NULL DEFAULT '',
    num_cre   TEXT NOT NULL DEFAULT '',
    val_des   INTEGER NOT NULL DEFAULT 0,
    tip_des   TEXT NOT NULL DEFAULT '',
    tab_dism  TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (reg_patr, num_afil) REFERENCES asegura(reg_patr, num_afil) ON UPDATE CASCADE ON DELETE RESTRICT
) STRICT;

CREATE INDEX movtos_trabajador ON movtos (reg_patr, num_afil, fec_inic);

CREATE TABLE incapacidades (
    id           INTEGER PRIMARY KEY,
    reg_pat      TEXT NOT NULL,
    num_afi      TEXT NOT NULL,
    fec_acc      TEXT NOT NULL DEFAULT '',
    fol_inc      TEXT NOT NULL,
    grupo_inc    TEXT NOT NULL DEFAULT '',
    rec_rev      TEXT NOT NULL DEFAULT '',
    consecuencia TEXT NOT NULL DEFAULT '',
    tip_rie      TEXT NOT NULL DEFAULT '',
    ram_seg      TEXT NOT NULL DEFAULT '',
    secuela      TEXT NOT NULL DEFAULT '',
    con_inc      TEXT NOT NULL DEFAULT '',
    dia_sub      INTEGER NOT NULL DEFAULT 0 CHECK (dia_sub >= 0),
    por_inc      INTEGER NOT NULL DEFAULT 0 CHECK (por_inc BETWEEN 0 AND 100),
    ind_def      TEXT NOT NULL DEFAULT '',
    fec_ter      TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (reg_pat, num_afi) REFERENCES asegura(reg_patr, num_afil) ON UPDATE CASCADE ON DELETE RESTRICT,
    UNIQUE (reg_pat, num_afi, fol_inc)
) STRICT;
