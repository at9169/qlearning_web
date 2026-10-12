// Q-learning con interfaccia Slint: versione per browser (WebAssembly) e desktop.
//
// Nel browser non esistono finestre separate: la griglia e la tabella Q sono
// due pannelli affiancati, separati da un divisore che si trascina col mouse.
// "Analisi" e "Info Q" si aprono come pannelli sovrapposti (tasto Chiudi).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};

// std::time::Instant non funziona in WebAssembly: web_time è identico, ma funziona anche nel browser
use web_time::Instant;

slint::slint! {
    import { Button, Slider, ListView, ScrollView } from "std-widgets.slint";

    // Un punto del grafico, in coordinate normalizzate 0..1 rispetto all'area del grafico
    export struct Punto {
        x: float,
        y: float,
    }

    // Una sezione della finestra di spiegazione dei valori Q
    export struct Sezione {
        titolo: string,
        testo: string,
    }

    // Una riga della tabella Q (una cella della griglia)
    export struct RigaQ {
        cella: string,
        v0: string,       // su
        v1: string,       // giù
        v2: string,       // sinistra
        v3: string,       // destra
        best: int,        // 0 = nessuna, 1..4 = azione con Q massimo
        freccia: string,  // simbolo dell'azione migliore
        corrente: bool,   // true se l'agente si trova in questa cella
    }

    // Una riga della tabella Q (intestazione, riga corrente e lista).
    // Le colonne hanno larghezze proporzionali, così si adattano al pannello.
    component RigaTabella inherits Rectangle {
        in property <RigaQ> d;
        property <length> w: root.width - 16px;
        height: 24px;

        HorizontalLayout {
            padding-left: 8px;
            padding-right: 8px;

            Text {
                text: root.d.cella;
                width: root.w * 0.20;
                overflow: elide;
                vertical-alignment: center;
                font-size: 13px;
                color: #222222;
            }
            Text {
                text: root.d.v0;
                width: root.w * 0.17;
                horizontal-alignment: right;
                vertical-alignment: center;
                font-size: 13px;
                font-weight: root.d.best == 1 ? 700 : 400;
                color: root.d.best == 1 ? #2e7d32 : #333333;
            }
            Text {
                text: root.d.v1;
                width: root.w * 0.17;
                horizontal-alignment: right;
                vertical-alignment: center;
                font-size: 13px;
                font-weight: root.d.best == 2 ? 700 : 400;
                color: root.d.best == 2 ? #2e7d32 : #333333;
            }
            Text {
                text: root.d.v2;
                width: root.w * 0.17;
                horizontal-alignment: right;
                vertical-alignment: center;
                font-size: 13px;
                font-weight: root.d.best == 3 ? 700 : 400;
                color: root.d.best == 3 ? #2e7d32 : #333333;
            }
            Text {
                text: root.d.v3;
                width: root.w * 0.17;
                horizontal-alignment: right;
                vertical-alignment: center;
                font-size: 13px;
                font-weight: root.d.best == 4 ? 700 : 400;
                color: root.d.best == 4 ? #2e7d32 : #333333;
            }
            Text {
                text: root.d.freccia;
                width: root.w * 0.12;
                horizontal-alignment: center;
                vertical-alignment: center;
                font-size: 13px;
                color: #1f4e79;
            }
        }
    }

    export component AppWindow inherits Window {
        title: "Q-learning";
        background: #e4e4e4;
        preferred-width: 1280px;
        preferred-height: 780px;
        min-width: 800px;
        min-height: 520px;

        // --- Griglia ---------------------------------------------------------
        in property <string> titolo-griglia: "Griglia";
        in property <int> righe: 5;
        in property <int> colonne: 8;

        // Stato di ogni cella: 0 = vuota, 1 = X (ostacolo), 2 = start, 3 = target
        in property <[int]> celle;

        // Frecce della politica: 0 = nessuna, 1 = su, 2 = giù, 3 = sinistra, 4 = destra
        in property <[int]> frecce;

        // Celle visitate dall'agente nell'episodio in corso (0/1)
        in property <[int]> scia;

        // Posizione dell'agente
        in property <bool> agente-visibile: false;
        in property <int> agente-r: 0;
        in property <int> agente-c: 0;

        // Fase: 0 = fermo, 1 = in corso, 2 = in pausa, 3 = completato
        in property <int> fase: 0;
        in property <string> info;

        // Cursore della velocità (0 = lentissima, 100 = massima)
        in-out property <float> velocita: 50;
        in property <string> desc-velocita;

        // Modo di inserimento: 0 = nessuno, 1 = ostacoli, 2 = start, 3 = target
        in-out property <int> modo: 0;

        in property <length> spazio: 2px;

        // --- Tabella Q -------------------------------------------------------
        in property <[RigaQ]> righe-q;
        in property <RigaQ> corrente;
        in property <bool> ha-corrente: false;
        in property <string> info-tabella;

        // --- Pannelli sovrapposti: 0 = nessuno, 1 = Info Q, 2 = Analisi ------------
        in-out property <int> overlay: 0;

        // Grafico di analisi
        in property <[Punto]> punti;
        in property <string> percorso-media;
        in property <string> percorso-griglia;
        in property <[string]> etichette-y;
        in property <[string]> etichette-x;
        in property <string> sottotitolo;
        in property <string> etichetta-media;

        // Spiegazione dei valori Q
        in property <string> formula;
        in property <[Sezione]> sezioni;

        // --- Divisore: frazione della larghezza occupata dal pannello sinistro -----
        in-out property <float> frazione: 0.5;
        property <length> sx: max(460px, min(root.width - 340px, root.width * root.frazione));

        callback cella-cliccata(int);
        callback impara();
        callback pausa();
        callback passo();
        callback reset();
        callback analisi();
        callback info-q();
        callback velocita-cambiata(float);

        // =====================================================================
        // Pannello sinistro: la griglia
        // =====================================================================
        Rectangle {
            x: 0px;
            y: 0px;
            width: root.sx;
            height: root.height;
            background: #f7f7f7;

            VerticalLayout {
                padding: 12px;
                spacing: 8px;

                Text {
                    text: root.titolo-griglia;
                    font-size: 18px;
                    font-weight: 700;
                    color: #222222;
                }

                GridLayout {
                    spacing: 6px;

                    Row {
                        Button {
                            text: "Metti ostacoli";
                            primary: root.modo == 1;
                            clicked => { root.modo = 1; }
                        }
                        Button {
                            text: "Fine ostacoli";
                            clicked => { root.modo = 0; }
                        }
                        Button {
                            text: "Start";
                            primary: root.modo == 2;
                            clicked => { root.modo = 2; }
                        }
                        Button {
                            text: "Target";
                            primary: root.modo == 3;
                            clicked => { root.modo = 3; }
                        }
                    }
                    Row {
                        Button {
                            text: "Impara";
                            primary: root.fase == 0 || root.fase == 3;
                            clicked => { root.impara(); }
                        }
                        Button {
                            text: root.fase == 2 ? "Riprendi" : "Pausa";
                            enabled: root.fase == 1 || root.fase == 2;
                            clicked => { root.pausa(); }
                        }
                        Button {
                            text: "Un passo";
                            enabled: root.fase == 2;
                            clicked => { root.passo(); }
                        }
                        Button {
                            text: "Reset";
                            enabled: root.fase != 0;
                            clicked => { root.reset(); }
                        }
                    }
                    Row {
                        Button {
                            text: "Analisi";
                            clicked => { root.analisi(); }
                        }
                        Button {
                            text: "Info Q";
                            clicked => { root.info-q(); }
                        }
                    }
                }

                // Cursore della velocità
                HorizontalLayout {
                    spacing: 8px;

                    Text {
                        text: "Velocità";
                        vertical-alignment: center;
                        font-size: 13px;
                        font-weight: 700;
                        color: #222222;
                    }
                    Text {
                        text: "lenta";
                        vertical-alignment: center;
                        font-size: 11px;
                        color: #555555;
                    }
                    Slider {
                        horizontal-stretch: 1;
                        minimum: 0;
                        maximum: 100;
                        value <=> root.velocita;
                        changed(v) => { root.velocita-cambiata(v); }
                    }
                    Text {
                        text: "massima";
                        vertical-alignment: center;
                        font-size: 11px;
                        color: #555555;
                    }
                    Text {
                        text: root.desc-velocita;
                        width: 120px;
                        horizontal-alignment: right;
                        vertical-alignment: center;
                        font-size: 12px;
                        color: #555555;
                    }
                }

                // Area della griglia: le celle si adattano allo spazio disponibile
                area := Rectangle {
                    vertical-stretch: 1;
                    background: #f0f0f0;
                    border-width: 1px;
                    border-color: #888888;
                    clip: true;

                    property <length> cella: max(14px, min(
                        (self.width - 16px - (root.colonne + 1) * root.spazio) / root.colonne,
                        (self.height - 16px - (root.righe + 1) * root.spazio) / root.righe));

                    Rectangle {
                        width: root.colonne * (area.cella + root.spazio) + root.spazio;
                        height: root.righe * (area.cella + root.spazio) + root.spazio;
                        x: (parent.width - self.width) / 2;
                        y: (parent.height - self.height) / 2;
                        background: #d0d0d0;

                        for r in root.righe: Rectangle {
                            for c in root.colonne: cell := Rectangle {
                                property <int> stato: root.celle[r * root.colonne + c];
                                property <int> freccia: root.frecce[r * root.colonne + c];
                                property <int> visitata: root.scia[r * root.colonne + c];

                                x: root.spazio + c * (area.cella + root.spazio);
                                y: root.spazio + r * (area.cella + root.spazio);
                                width: area.cella;
                                height: area.cella;
                                background: self.visitata == 1 ? #7f9bbd : #808080;

                                // X (ostacolo)
                                if cell.stato == 1: Path {
                                    width: parent.width;
                                    height: parent.height;
                                    viewbox-width: 100;
                                    viewbox-height: 100;
                                    commands: "M 20 20 L 80 80 M 80 20 L 20 80";
                                    stroke: #c00000;
                                    stroke-width: 4px;
                                }

                                // Pallino "start"
                                if cell.stato == 2: Rectangle {
                                    width: parent.width;
                                    height: parent.height;
                                    Rectangle {
                                        width: area.cella * 0.29;
                                        height: self.width;
                                        border-radius: self.width / 2;
                                        background: #2e7d32;
                                        x: (parent.width - self.width) / 2;
                                        y: area.cella * 0.17;
                                    }
                                    Text {
                                        text: "start";
                                        font-size: max(8px, area.cella * 0.2);
                                        color: white;
                                        width: parent.width;
                                        y: area.cella * 0.54;
                                        horizontal-alignment: center;
                                    }
                                }

                                // Pallino "target"
                                if cell.stato == 3: Rectangle {
                                    width: parent.width;
                                    height: parent.height;
                                    Rectangle {
                                        width: area.cella * 0.29;
                                        height: self.width;
                                        border-radius: self.width / 2;
                                        background: #1565c0;
                                        x: (parent.width - self.width) / 2;
                                        y: area.cella * 0.17;
                                    }
                                    Text {
                                        text: "target";
                                        font-size: max(8px, area.cella * 0.2);
                                        color: white;
                                        width: parent.width;
                                        y: area.cella * 0.54;
                                        horizontal-alignment: center;
                                    }
                                }

                                // Freccia della politica (solo sulle celle libere)
                                if cell.stato == 0 && cell.freccia > 0: Path {
                                    width: parent.width;
                                    height: parent.height;
                                    viewbox-width: 100;
                                    viewbox-height: 100;
                                    commands:
                                        cell.freccia == 1 ? "M 50 80 L 50 20 M 30 40 L 50 20 L 70 40" :
                                        cell.freccia == 2 ? "M 50 20 L 50 80 M 30 60 L 50 80 L 70 60" :
                                        cell.freccia == 3 ? "M 80 50 L 20 50 M 40 30 L 20 50 L 40 70" :
                                                            "M 20 50 L 80 50 M 60 30 L 80 50 L 60 70";
                                    stroke: #ffd54f;
                                    stroke-width: 5px;
                                    stroke-line-cap: round;
                                    stroke-line-join: round;
                                }

                                TouchArea {
                                    clicked => { root.cella-cliccata(r * root.colonne + c); }
                                }
                            }
                        }

                        // L'agente: pallino arancione sopra a tutte le celle
                        if root.agente-visibile: Rectangle {
                            x: root.spazio + root.agente-c * (area.cella + root.spazio) + area.cella * 0.23;
                            y: root.spazio + root.agente-r * (area.cella + root.spazio) + area.cella * 0.23;
                            width: area.cella * 0.54;
                            height: self.width;
                            border-radius: self.width / 2;
                            background: #ff9800;
                            border-width: 2px;
                            border-color: white;
                        }
                    }
                }

                Text {
                    text: root.info;
                    font-size: 13px;
                    wrap: word-wrap;
                    color: #222222;
                }
            }
        }

        // =====================================================================
        // Divisore trascinabile
        // =====================================================================
        Rectangle {
            x: root.sx;
            y: 0px;
            width: 8px;
            height: root.height;
            background: divisore.has-hover || divisore.pressed ? #7f9bbd : #b8b8b8;

            Rectangle {
                width: 2px;
                height: 40px;
                border-radius: 1px;
                background: white;
                x: (parent.width - self.width) / 2;
                y: (parent.height - self.height) / 2;
            }

            divisore := TouchArea {
                mouse-cursor: MouseCursor.col-resize;
                moved => {
                    root.frazione = clamp((root.sx + self.mouse-x - self.pressed-x) / root.width, 0.1, 0.9);
                }
            }
        }

        // =====================================================================
        // Pannello destro: la tabella Q
        // =====================================================================
        Rectangle {
            x: root.sx + 8px;
            y: 0px;
            width: root.width - root.sx - 8px;
            height: root.height;
            background: white;

            VerticalLayout {
                padding: 12px;
                spacing: 6px;

                Text {
                    text: "Tabella Q (celle libere e start)";
                    font-size: 18px;
                    font-weight: 700;
                    color: #222222;
                }
                Text {
                    text: root.info-tabella;
                    font-size: 12px;
                    wrap: word-wrap;
                    color: #555555;
                }

                // Intestazione
                RigaTabella {
                    background: #dde6ee;
                    d: {
                        cella: "Cella",
                        v0: "su",
                        v1: "giù",
                        v2: "sinistra",
                        v3: "destra",
                        best: 0,
                        freccia: "migliore",
                        corrente: false,
                    };
                }

                // Riga della cella in cui si trova l'agente in questo momento
                if root.ha-corrente: RigaTabella {
                    background: #ffcc80;
                    d: root.corrente;
                }

                ListView {
                    vertical-stretch: 1;
                    for riga in root.righe-q: RigaTabella {
                        d: riga;
                        background: riga.corrente ? #fff3c4 : white;
                    }
                }
            }
        }

        // =====================================================================
        // Pannello sovrapposto: Analisi (2) oppure Info Q (1)
        // =====================================================================
        if root.overlay != 0: ov := Rectangle {
            x: 0px;
            y: 0px;
            width: root.width;
            height: root.height;
            background: white;

            // Posizione e dimensioni dell'area del grafico
            property <length> ax: 80px;
            property <length> ay: 60px;
            property <length> aw: max(300px, root.width - 120px);
            property <length> ah: max(160px, root.height - 170px);

            // Impedisce che i click arrivino ai pannelli sotto
            TouchArea { }

            Button {
                text: "Chiudi";
                x: parent.width - self.width - 16px;
                y: 12px;
                clicked => { root.overlay = 0; }
            }

            // ------------------------- Analisi ---------------------------------
            if root.overlay == 2: Rectangle {
                x: 0px;
                y: 0px;
                width: parent.width;
                height: parent.height;

                Text {
                    x: ov.ax;
                    y: 10px;
                    text: "Ricompensa totale per episodio";
                    font-size: 18px;
                    font-weight: 700;
                    color: #222222;
                }

                Text {
                    x: ov.ax;
                    y: 36px;
                    text: root.sottotitolo;
                    font-size: 12px;
                    color: #555555;
                }

                // Griglia
                Path {
                    x: ov.ax;
                    y: ov.ay;
                    width: ov.aw;
                    height: ov.ah;
                    viewbox-width: 1000;
                    viewbox-height: 500;
                    commands: root.percorso-griglia;
                    stroke: #dddddd;
                    stroke-width: 1px;
                }

                // Un puntino semitrasparente per ogni episodio
                Rectangle {
                    x: ov.ax;
                    y: ov.ay;
                    width: ov.aw;
                    height: ov.ah;

                    for p in root.punti: Rectangle {
                        x: p.x * ov.aw - 2px;
                        y: p.y * ov.ah - 2px;
                        width: 4px;
                        height: 4px;
                        border-radius: 2px;
                        background: #1f77b4.with-alpha(0.25);
                    }
                }

                // Media mobile (rossa)
                Path {
                    x: ov.ax;
                    y: ov.ay;
                    width: ov.aw;
                    height: ov.ah;
                    viewbox-width: 1000;
                    viewbox-height: 500;
                    commands: root.percorso-media;
                    stroke: #c00000;
                    stroke-width: 2px;
                }

                // Cornice
                Rectangle {
                    x: ov.ax;
                    y: ov.ay;
                    width: ov.aw;
                    height: ov.ah;
                    border-width: 1px;
                    border-color: #999999;
                }

                // Etichette asse Y
                for etichetta[k] in root.etichette-y: Text {
                    x: 0px;
                    y: ov.ay + ov.ah * k / (root.etichette-y.length - 1) - 8px;
                    width: ov.ax - 8px;
                    horizontal-alignment: right;
                    text: etichetta;
                    font-size: 11px;
                    color: #333333;
                }

                // Etichette asse X
                for etichetta[k] in root.etichette-x: Text {
                    x: ov.ax + ov.aw * k / (root.etichette-x.length - 1) - 30px;
                    y: ov.ay + ov.ah + 6px;
                    width: 60px;
                    horizontal-alignment: center;
                    text: etichetta;
                    font-size: 11px;
                    color: #333333;
                }

                Text {
                    x: ov.ax;
                    y: ov.ay + ov.ah + 28px;
                    width: ov.aw;
                    horizontal-alignment: center;
                    text: "Episodio";
                    font-size: 12px;
                    color: #333333;
                }

                // Legenda
                Rectangle {
                    x: ov.ax + 8px;
                    y: ov.ay + ov.ah + 66px;
                    width: 6px;
                    height: 6px;
                    border-radius: 3px;
                    background: #1f77b4.with-alpha(0.6);
                }
                Text {
                    x: ov.ax + 30px;
                    y: ov.ay + ov.ah + 61px;
                    text: "Ricompensa dell'episodio";
                    font-size: 12px;
                    color: #333333;
                }
                Rectangle {
                    x: ov.ax + 210px;
                    y: ov.ay + ov.ah + 68px;
                    width: 24px;
                    height: 3px;
                    background: #c00000;
                }
                Text {
                    x: ov.ax + 240px;
                    y: ov.ay + ov.ah + 61px;
                    text: root.etichetta-media;
                    font-size: 12px;
                    color: #333333;
                }
            }

            // ------------------------- Info Q ---------------------------------
            if root.overlay == 1: ScrollView {
                x: 0px;
                y: 52px;
                width: parent.width;
                height: parent.height - 52px;

                VerticalLayout {
                    width: ov.width - 24px;
                    padding: 20px;
                    spacing: 12px;
                    alignment: start;

                    Text {
                        text: "Come vengono valutati i valori Q";
                        font-size: 18px;
                        font-weight: 700;
                        color: #222222;
                    }

                    Rectangle {
                        background: #f3f3f3;
                        border-radius: 4px;
                        height: 40px;

                        Text {
                            text: root.formula;
                            font-size: 14px;
                            color: #222222;
                            horizontal-alignment: center;
                            vertical-alignment: center;
                        }
                    }

                    for s in root.sezioni: VerticalLayout {
                        spacing: 4px;

                        Text {
                            text: s.titolo;
                            font-size: 14px;
                            font-weight: 700;
                            color: #1f4e79;
                        }
                        Text {
                            text: s.testo;
                            font-size: 13px;
                            wrap: word-wrap;
                            color: #333333;
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Parametri del Q-learning (modificali qui: GUI e spiegazione si aggiornano da soli)
// ---------------------------------------------------------------------------

const ALFA: f64 = 0.1; // quanto pesa l'informazione nuova
const GAMMA: f64 = 0.9; // quanto contano le ricompense future
const EPSILON: f64 = 0.2; // probabilità di azione casuale
const EPISODI: usize = 5000;
const MAX_PASSI: usize = 200;

// Stampa la tabella Q a terminale ogni SNAPSHOT_OGNI episodi, per N_SNAPSHOT volte
const SNAPSHOT_OGNI: usize = 10;
const N_SNAPSHOT: usize = 5;

// Ricompense
const R_GOAL: f64 = 10.0; // raggiunge il Target
const R_OSTACOLO: f64 = -5.0; // urta un ostacolo
const R_BORDO: f64 = -5.0; // prova a uscire dalla griglia (metti 0.0 per non penalizzarlo)

// Con il cursore della velocità al massimo, il programma lavora a blocchi di
// questa durata (in ms) tra un aggiornamento della grafica e il successivo
const BUDGET_MS: u64 = 12;

// ---------------------------------------------------------------------------
// Q-learning
// ---------------------------------------------------------------------------

const N_AZIONI: usize = 4;

// su, giù, sinistra, destra
// (il codice freccia nella GUI è l'indice dell'azione + 1)
const AZIONI: [(i32, i32); N_AZIONI] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

type Stato = (usize, usize);

// Tabella Q: una riga per cella (indice = riga * m + colonna)
type TabellaQ = Vec<[f64; N_AZIONI]>;

// Generatore di numeri casuali xorshift (nessuna dipendenza esterna)
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    // numero tra 0 e 1
    fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    // intero tra 0 e n-1
    fn sotto(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

// L'ambiente, costruito a partire dalla griglia disegnata nella GUI
struct Ambiente {
    n: usize,
    m: usize,
    start: Stato,
    goal: Stato,
    ostacoli: Vec<bool>, // indice = riga * m + colonna
}

impl Ambiente {
    // Legge ostacoli, start e target dal modello delle celle della GUI
    fn da_celle(celle: &VecModel<i32>, n: usize, m: usize) -> Result<Self, String> {
        let mut start = None;
        let mut goal = None;
        let mut ostacoli = vec![false; n * m];

        for (i, valore) in celle.iter().enumerate() {
            let pos = (i / m, i % m);
            match valore {
                1 => ostacoli[i] = true,
                2 => start = Some(pos),
                3 => goal = Some(pos),
                _ => {}
            }
        }

        match (start, goal) {
            (Some(start), Some(goal)) => Ok(Ambiente { n, m, start, goal, ostacoli }),
            (None, _) => Err("manca lo Start".to_string()),
            (_, None) => Err("manca il Target".to_string()),
        }
    }

    fn indice(&self, s: Stato) -> usize {
        s.0 * self.m + s.1
    }

    // Dato stato e azione, restituisce (s', R, fine)
    fn passo(&self, s: Stato, a: usize) -> (Stato, f64, bool) {
        let nr = s.0 as i32 + AZIONI[a].0;
        let nc = s.1 as i32 + AZIONI[a].1;

        if nr < 0 || nr >= self.n as i32 || nc < 0 || nc >= self.m as i32 {
            return (s, R_BORDO, false); // bordo: resta fermo, penalità
        }
        let nuovo = (nr as usize, nc as usize);

        if self.ostacoli[self.indice(nuovo)] {
            return (s, R_OSTACOLO, false); // ostacolo: resta fermo, penalità
        }
        if nuovo == self.goal {
            return (nuovo, R_GOAL, true); // goal: fine episodio
        }
        (nuovo, 0.0, false)
    }
}

// Azione con Q massimo; a parità sceglie a caso
fn migliore(q: &TabellaQ, env: &Ambiente, s: Stato, rng: &mut Rng) -> usize {
    let valori = &q[env.indice(s)];
    let max = valori.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let candidati: Vec<usize> = (0..N_AZIONI).filter(|&i| valori[i] == max).collect();
    candidati[rng.sotto(candidati.len())]
}

// Azione con Q massimo in modo deterministico (per la grafica):
// None se tutti i valori sono uguali, cioè se la cella non è stata ancora appresa
fn miglior_indice(v: &[f64; N_AZIONI]) -> Option<usize> {
    if v.iter().all(|&x| x == v[0]) {
        return None;
    }
    let mut best = 0;
    for k in 1..N_AZIONI {
        if v[k] > v[best] {
            best = k;
        }
    }
    Some(best)
}

// epsilon-greedy
fn scegli_azione(q: &TabellaQ, env: &Ambiente, s: Stato, epsilon: f64, rng: &mut Rng) -> usize {
    if rng.f64() < epsilon {
        rng.sotto(N_AZIONI)
    } else {
        migliore(q, env, s, rng)
    }
}

// ---------------------------------------------------------------------------
// Sessione di addestramento: avanza un passo alla volta, così la GUI può
// mostrare l'agente in tempo reale e il cursore può rallentare tutto
// ---------------------------------------------------------------------------

struct Sessione {
    env: Ambiente,
    q: TabellaQ,
    rng: Rng,
    ricompense: Vec<f64>, // ricompensa totale degli episodi già conclusi
    s: Stato,             // posizione attuale dell'agente
    totale: f64,          // ricompensa accumulata nell'episodio in corso
    passo: usize,         // passi fatti nell'episodio in corso
    scia: Vec<bool>,      // celle visitate nell'episodio in corso
    concluso: bool,       // l'episodio è terminato ma non ancora "chiuso"
    finito: bool,         // tutti gli episodi sono stati fatti
    in_pausa: bool,
    riepilogo_stampato: bool,
    mappa_righe: Vec<usize>, // celle mostrate nella tabella Q (ostacoli e target esclusi)
}

impl Sessione {
    fn nuova(env: Ambiente) -> Self {
        let q: TabellaQ = vec![[0.0; N_AZIONI]; env.n * env.m];

        // Tabella iniziale: tutti i valori sono 0
        stampa_tabella_q("Tabella Q iniziale (episodio 0)", &q, &env);

        let mappa_righe: Vec<usize> = (0..env.n * env.m)
            .filter(|&i| !env.ostacoli[i] && (i / env.m, i % env.m) != env.goal)
            .collect();

        let mut scia = vec![false; env.n * env.m];
        scia[env.indice(env.start)] = true;

        Sessione {
            s: env.start,
            env,
            q,
            rng: Rng(0x2545F4914F6CDD1D),
            ricompense: Vec::with_capacity(EPISODI),
            totale: 0.0,
            passo: 0,
            scia,
            concluso: false,
            finito: false,
            in_pausa: false,
            riepilogo_stampato: false,
            mappa_righe,
        }
    }

    // Fa avanzare l'addestramento di un singolo passo.
    // Quando un episodio finisce, l'agente resta per un passo sulla cella
    // finale (così si vede) e il passo successivo chiude l'episodio.
    fn avanza(&mut self) {
        if self.finito {
            return;
        }
        if self.concluso {
            self.chiudi_episodio();
            return;
        }

        let a = scegli_azione(&self.q, &self.env, self.s, EPSILON, &mut self.rng);
        let (s_nuovo, r, fine) = self.env.passo(self.s, a);
        self.totale += r;

        // aggiornamento Q-learning
        let max_q = if fine {
            0.0
        } else {
            self.q[self.env.indice(s_nuovo)]
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max)
        };
        let i = self.env.indice(self.s);
        let vecchio = self.q[i][a];
        self.q[i][a] = vecchio + ALFA * (r + GAMMA * max_q - vecchio);

        self.s = s_nuovo;
        self.passo += 1;
        let j = self.env.indice(s_nuovo);
        self.scia[j] = true;

        if fine || self.passo >= MAX_PASSI {
            self.concluso = true;
        }
    }

    fn chiudi_episodio(&mut self) {
        self.ricompense.push(self.totale);

        // Istantanea della tabella ogni SNAPSHOT_OGNI episodi, per N_SNAPSHOT volte
        let fatti = self.ricompense.len();
        if fatti % SNAPSHOT_OGNI == 0 && fatti / SNAPSHOT_OGNI <= N_SNAPSHOT {
            stampa_tabella_q(&format!("Tabella Q dopo {} episodi", fatti), &self.q, &self.env);
        }

        if fatti >= EPISODI {
            self.finito = true;
            return;
        }

        // Nuovo episodio: l'agente torna allo Start
        self.s = self.env.start;
        self.totale = 0.0;
        self.passo = 0;
        self.concluso = false;
        for v in self.scia.iter_mut() {
            *v = false;
        }
        let i = self.env.indice(self.s);
        self.scia[i] = true;
    }

    // Stampa percorso e tabella finale a terminale (una sola volta)
    fn riepilogo(&mut self) {
        if self.finito && !self.riepilogo_stampato {
            self.riepilogo_stampato = true;
            stampa_percorso(&self.q, &self.env, &mut self.rng);
            stampa_tabella_q("Tabella Q finale", &self.q, &self.env);
        }
    }

    // 0 = fermo, 1 = in corso, 2 = in pausa, 3 = completato
    fn fase(&self) -> i32 {
        if self.finito {
            3
        } else if self.in_pausa {
            2
        } else {
            1
        }
    }
}

// Gioca una partita con epsilon = 0 e stampa il percorso nel terminale
fn stampa_percorso(q: &TabellaQ, env: &Ambiente, rng: &mut Rng) {
    let mut s = env.start;
    let mut percorso = vec![s];
    for _ in 0..50 {
        let a = migliore(q, env, s, rng);
        let (s_nuovo, _, fine) = env.passo(s, a);
        s = s_nuovo;
        percorso.push(s);
        if fine {
            break;
        }
    }
    println!("Percorso greedy ({} celle):", percorso.len());
    println!("{:?}", percorso);
    if s == env.goal {
        println!("Goal raggiunto!");
    } else {
        println!("Goal NON raggiunto.");
    }
}

// Stampa la tabella Q: una riga per cella, con i valori delle 4 azioni
fn stampa_tabella_q(titolo: &str, q: &TabellaQ, env: &Ambiente) {
    const NOMI: [&str; N_AZIONI] = ["su", "giù", "sinistra", "destra"];
    const FRECCE: [char; N_AZIONI] = ['^', 'v', '<', '>'];

    println!();
    println!("{} (celle libere e start):", titolo);
    println!(
        "{:<9} {:>10} {:>10} {:>10} {:>10}  {}",
        "cella", NOMI[0], NOMI[1], NOMI[2], NOMI[3], "migliore"
    );

    for r in 0..env.n {
        for c in 0..env.m {
            let s = (r, c);
            let i = env.indice(s);

            // Ostacoli e target non vengono mai aggiornati (Q sempre 0): li salto
            if env.ostacoli[i] || s == env.goal {
                continue;
            }

            let v = &q[i];

            // Se tutti i valori sono uguali la cella non è stata appresa: nessuna freccia
            let simbolo = match miglior_indice(v) {
                Some(k) => FRECCE[k],
                None => '-',
            };

            let etichetta = if s == env.start {
                format!("({},{}) S", r, c)
            } else {
                format!("({},{})", r, c)
            };

            println!(
                "{:<9} {:>10.3} {:>10.3} {:>10.3} {:>10.3}  {}",
                etichetta, v[0], v[1], v[2], v[3], simbolo
            );
        }
    }
}


// ---------------------------------------------------------------------------
// Collegamento con la grafica
// ---------------------------------------------------------------------------

// Tutto ciò che serve per aggiornare l'interfaccia
#[derive(Clone)]
struct Ui {
    finestra: slint::Weak<AppWindow>,
    frecce: Rc<VecModel<i32>>,
    scia: Rc<VecModel<i32>>,
    righe_q: Rc<VecModel<RigaQ>>,
}

// Riga della tabella Q per la cella `pos`
fn riga_q(env: &Ambiente, q: &TabellaQ, pos: Stato, corrente: bool) -> RigaQ {
    const SIMBOLI: [&str; N_AZIONI] = ["↑", "↓", "←", "→"];

    let v = &q[env.indice(pos)];
    let best = miglior_indice(v);

    let mut cella = format!("({},{})", pos.0, pos.1);
    if pos == env.start {
        cella.push_str(" S");
    } else if pos == env.goal {
        cella.push_str(" T");
    }

    RigaQ {
        cella: cella.into(),
        v0: format!("{:.2}", v[0]).into(),
        v1: format!("{:.2}", v[1]).into(),
        v2: format!("{:.2}", v[2]).into(),
        v3: format!("{:.2}", v[3]).into(),
        best: best.map_or(0, |k| k as i32 + 1),
        freccia: best.map_or("-", |k| SIMBOLI[k]).into(),
        corrente,
    }
}

// Testo di stato mostrato sotto la griglia e sopra la tabella Q
fn testo_stato(s: &Sessione) -> String {
    let n = s.ricompense.len();
    let ultimi = &s.ricompense[n.saturating_sub(FINESTRA_MA)..];
    let media = if ultimi.is_empty() {
        String::from("-")
    } else {
        format!("{:.2}", ultimi.iter().sum::<f64>() / ultimi.len() as f64)
    };

    if s.finito {
        format!(
            "Completato: {} episodi  |  media ultimi {}: {}",
            n,
            ultimi.len(),
            media
        )
    } else {
        format!(
            "Episodio {} / {}  |  passo {} / {}  |  ricompensa episodio: {:.0}  |  media ultimi {}: {}{}",
            n + 1,
            EPISODI,
            s.passo,
            MAX_PASSI,
            s.totale,
            ultimi.len(),
            media,
            if s.in_pausa { "  |  IN PAUSA" } else { "" }
        )
    }
}


// Porta nell'interfaccia lo stato attuale della sessione
fn aggiorna_ui(ui: &Ui, sess: &Sessione) {
    let env = &sess.env;

    // Frecce della politica e scia dell'episodio (solo se cambiano)
    for i in 0..env.n * env.m {
        let pos = (i / env.m, i % env.m);
        let freccia = if env.ostacoli[i] || pos == env.goal {
            0
        } else {
            miglior_indice(&sess.q[i]).map_or(0, |k| k as i32 + 1)
        };
        if ui.frecce.row_data(i) != Some(freccia) {
            ui.frecce.set_row_data(i, freccia);
        }

        let visitata = sess.scia[i] as i32;
        if ui.scia.row_data(i) != Some(visitata) {
            ui.scia.set_row_data(i, visitata);
        }
    }

    // Tabella Q
    for (k, &i) in sess.mappa_righe.iter().enumerate() {
        let pos = (i / env.m, i % env.m);
        ui.righe_q.set_row_data(k, riga_q(env, &sess.q, pos, pos == sess.s));
    }

    let testo = testo_stato(sess);

    if let Some(w) = ui.finestra.upgrade() {
        w.set_agente_r(sess.s.0 as i32);
        w.set_agente_c(sess.s.1 as i32);
        w.set_agente_visibile(true);
        w.set_fase(sess.fase());
        w.set_info(testo.clone().into());

        w.set_corrente(riga_q(env, &sess.q, sess.s, true));
        w.set_ha_corrente(true);
        w.set_info_tabella(testo.into());
    }
}

// Riporta l'interfaccia allo stato iniziale (nessun addestramento)
fn azzera_ui(ui: &Ui) {
    for i in 0..ui.frecce.row_count() {
        ui.frecce.set_row_data(i, 0);
        ui.scia.set_row_data(i, 0);
    }
    ui.righe_q.set_vec(Vec::new());

    if let Some(w) = ui.finestra.upgrade() {
        w.set_agente_visibile(false);
        w.set_fase(0);
        w.set_info("".into());
        w.set_ha_corrente(false);
        w.set_info_tabella("Nessun addestramento: premi \"Impara\" nel pannello della griglia.".into());
    }
}

// ---------------------------------------------------------------------------
// Velocità
// ---------------------------------------------------------------------------

// Dal valore del cursore (0..=100) ricava:
// (intervallo del timer in ms, passi per ogni scatto, modalità massima, descrizione)
//   0..=60  : 1 passo per scatto, da 1 passo/s a 100 passi/s
//   60..100 : scatto ogni 10 ms con un numero crescente di passi
//   100     : nessun limite (a blocchi di BUDGET_MS)
fn parametri_velocita(v: f64) -> (u64, usize, bool, String) {
    if v >= 99.5 {
        return (1, 0, true, "massima (senza limiti)".to_string());
    }

    let (ms, passi) = if v <= 60.0 {
        (1000.0 * 0.01f64.powf(v / 60.0), 1usize)
    } else {
        let p = 10f64.powf((v - 60.0) / 40.0 * 2.3).round().max(1.0);
        (10.0, p as usize)
    };

    let ms = ms.round().max(1.0) as u64;
    let al_secondo = passi as f64 * 1000.0 / ms as f64;
    let desc = if al_secondo < 10.0 {
        format!("{:.1} passi/s", al_secondo)
    } else {
        format!("{:.0} passi/s", al_secondo)
    };
    (ms, passi, false, desc)
}

// ---------------------------------------------------------------------------
// Finestra "Info Q"
// ---------------------------------------------------------------------------

// Riempie la finestrina con la spiegazione; i numeri vengono dalle costanti
fn imposta_info_q(w: &AppWindow) {
    w.set_formula("Q(s,a)  ←  Q(s,a) + α · ( r + γ · max Q(s') − Q(s,a) )".into());

    let sezioni = vec![
        Sezione {
            titolo: "Cos'è il valore Q".into(),
            testo: "Per ogni cella della griglia e per ognuna delle 4 azioni (su, giù, sinistra, \
                    destra) il programma tiene un numero: il valore Q. Indica quanto è promettente \
                    fare quella mossa partendo da quella cella. All'inizio sono tutti 0 e vengono \
                    corretti mossa dopo mossa."
                .into(),
        },
        Sezione {
            titolo: "Come viene aggiornato".into(),
            testo: format!(
                "Dopo ogni singola mossa l'agente aggiorna il valore Q della cella da cui è \
                 partito, con la formula qui sopra:\n\
                 • s: la cella di partenza, a: l'azione scelta, s': la cella in cui arriva\n\
                 • r: la ricompensa immediata: {:+.0} al Target, {:+.0} contro un ostacolo, \
                 {:+.0} contro il bordo (l'agente resta fermo), 0 in tutti gli altri casi\n\
                 • max Q(s'): il valore della mossa migliore dalla cella di arrivo \
                 (vale 0 se s' è il Target, perché l'episodio finisce)\n\
                 • α = {}: quanto pesa l'informazione nuova rispetto a quella già in tabella\n\
                 • γ = {}: quanto contano le ricompense future. Il {:+.0} del Target si propaga \
                 all'indietro: ogni cella più lontana vale il {:.0}% di quella successiva",
                R_GOAL,
                R_OSTACOLO,
                R_BORDO,
                ALFA,
                GAMMA,
                R_GOAL,
                GAMMA * 100.0
            )
            .into(),
        },
        Sezione {
            titolo: "Come sceglie la mossa (epsilon-greedy)".into(),
            testo: format!(
                "A ogni passo, con probabilità ε = {} ({:.0}%) l'agente fa un'azione casuale \
                 (esplorazione); altrimenti sceglie l'azione con il valore Q più alto nella cella \
                 in cui si trova (sfruttamento). A parità di valori sceglie a caso: all'inizio, \
                 con la tabella tutta a zero, le prime mosse sono quindi casuali.",
                EPSILON,
                EPSILON * 100.0
            )
            .into(),
        },
        Sezione {
            titolo: "Cos'è un episodio".into(),
            testo: format!(
                "Ogni episodio parte dalla cella Start e finisce quando l'agente raggiunge il \
                 Target oppure dopo {} passi. La tabella non viene azzerata tra un episodio e \
                 l'altro: ciò che impara in un episodio viene usato nei successivi. In tutto gli \
                 episodi sono {}.",
                MAX_PASSI, EPISODI
            )
            .into(),
        },
        Sezione {
            titolo: "Ricompensa di un episodio".into(),
            testo: "È la somma delle ricompense ottenute in tutti i passi dell'episodio. È il \
                    valore che compare come puntino nel grafico di Analisi."
                .into(),
        },
    ];
    w.set_sezioni(ModelRc::new(VecModel::from(sezioni)));
}

// ---------------------------------------------------------------------------
// Grafico di analisi
// ---------------------------------------------------------------------------

// Dimensioni dell'area del grafico in pixel: cambiale qui per ingrandire o rimpicciolire
const GW: f64 = 1000.0;
const GH: f64 = 500.0;

// Numero di intervalli della griglia sull'asse X
const TACCHE_X: usize = 10;

// Ampiezza della media mobile, in episodi
const FINESTRA_MA: usize = 100;

// Media mobile "trailing": i primi punti usano meno di FINESTRA_MA episodi
fn media_mobile(v: &[f64], w: usize) -> Vec<f64> {
    let mut out = Vec::with_capacity(v.len());
    let mut somma = 0.0;
    for i in 0..v.len() {
        somma += v[i];
        if i >= w {
            somma -= v[i - w];
        }
        let k = (i + 1).min(w);
        out.push(somma / k as f64);
    }
    out
}

// Posizione orizzontale (in pixel) dell'episodio i su n
fn pos_x(i: usize, n: usize) -> f64 {
    if n > 1 {
        i as f64 / (n - 1) as f64 * GW
    } else {
        0.0
    }
}

// Posizione verticale (in pixel) di un valore, con l'asse Y da ymin a ymax
fn pos_y(val: f64, ymin: f64, ymax: f64) -> f64 {
    GH - (val - ymin) / (ymax - ymin) * GH
}

// Converte una serie di valori in comandi di tracciato (coordinate GW x GH)
fn path_serie(v: &[f64], ymin: f64, ymax: f64) -> String {
    use std::fmt::Write;

    let n = v.len();
    let mut s = String::new();
    for (i, &val) in v.iter().enumerate() {
        let cmd = if i == 0 { 'M' } else { 'L' };
        let _ = write!(s, "{} {:.1} {:.1} ", cmd, pos_x(i, n), pos_y(val, ymin, ymax));
    }
    s
}

// Griglia: 5 linee orizzontali e TACCHE_X + 1 verticali, equispaziate
fn path_griglia() -> String {
    use std::fmt::Write;

    let mut s = String::new();
    for k in 0..=4 {
        let y = GH * k as f64 / 4.0;
        let _ = write!(s, "M 0 {:.1} L {:.1} {:.1} ", y, GW, y);
    }
    for k in 0..=TACCHE_X {
        let x = GW * k as f64 / TACCHE_X as f64;
        let _ = write!(s, "M {:.1} 0 L {:.1} {:.1} ", x, x, GH);
    }
    s
}

// Riempie la finestra del grafico con i dati delle ricompense
fn imposta_grafico(g: &AppWindow, ricompense: &[f64]) {
    let n = ricompense.len();
    let media = media_mobile(ricompense, FINESTRA_MA);

    // Limiti dell'asse Y: multipli di 5, con 4 intervalli uguali
    let min = ricompense.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = ricompense.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let ymin = (min / 5.0).floor() * 5.0;
    let span = (((max - ymin) / 20.0).ceil() * 20.0).max(20.0);
    let ymax = ymin + span;

    // Un puntino per ogni episodio, in coordinate normalizzate 0..1 (il grafico si adatta al pannello)
    let punti: Vec<Punto> = ricompense
        .iter()
        .enumerate()
        .map(|(i, &val)| Punto {
            x: (pos_x(i, n) / GW) as f32,
            y: (pos_y(val, ymin, ymax) / GH) as f32,
        })
        .collect();
    g.set_punti(ModelRc::new(VecModel::from(punti)));

    g.set_percorso_media(path_serie(&media, ymin, ymax).into());
    g.set_percorso_griglia(path_griglia().into());

    // Etichette Y: dall'alto (ymax) al basso (ymin)
    let etichette_y: Vec<SharedString> = (0..=4)
        .map(|k| format!("{:.0}", ymax - span * k as f64 / 4.0).into())
        .collect();
    g.set_etichette_y(ModelRc::new(VecModel::from(etichette_y)));

    // Etichette X: numero di episodio, da 0 a n
    let etichette_x: Vec<SharedString> = (0..=TACCHE_X)
        .map(|k| format!("{}", n * k / TACCHE_X).into())
        .collect();
    g.set_etichette_x(ModelRc::new(VecModel::from(etichette_x)));

    // Riepilogo: media degli ultimi episodi, minimo e massimo
    let ultimi = &ricompense[n.saturating_sub(FINESTRA_MA)..];
    let media_ultimi = ultimi.iter().sum::<f64>() / ultimi.len() as f64;
    g.set_sottotitolo(
        format!(
            "{} episodi  |  media ultimi {}: {:.2}  |  min {:.0}  |  max {:.0}",
            n,
            ultimi.len(),
            media_ultimi,
            min,
            max
        )
        .into(),
    );
    g.set_etichetta_media(format!("Media mobile ({} episodi)", FINESTRA_MA).into());
}


// ---------------------------------------------------------------------------
// avvia: costruisce l'interfaccia e fa partire tutto
// (chiamata da main.rs sul desktop e da start() nel browser)
// ---------------------------------------------------------------------------

pub fn avvia() -> Result<(), slint::PlatformError> {
    let finestra = AppWindow::new()?;

    // Dimensioni della griglia: N righe, M colonne
    let n: i32 = 9;
    let m: i32 = 9;
    let celle_tot = (n * m) as usize;

    // Tutte le celle partono vuote (0), nessuna freccia, nessuna scia
    let celle = Rc::new(VecModel::from(vec![0i32; celle_tot]));
    let frecce = Rc::new(VecModel::from(vec![0i32; celle_tot]));
    let scia = Rc::new(VecModel::from(vec![0i32; celle_tot]));
    let righe_q: Rc<VecModel<RigaQ>> = Rc::new(VecModel::default());

    finestra.set_righe(n);
    finestra.set_colonne(m);
    finestra.set_celle(ModelRc::from(celle.clone()));
    finestra.set_frecce(ModelRc::from(frecce.clone()));
    finestra.set_scia(ModelRc::from(scia.clone()));
    finestra.set_righe_q(ModelRc::from(righe_q.clone()));

    finestra.set_titolo_griglia(format!("Griglia {}x{}", n, m).into());
    finestra.set_info_tabella("Nessun addestramento: premi \"Impara\" nel pannello della griglia.".into());

    let ui = Ui {
        finestra: finestra.as_weak(),
        frecce: frecce.clone(),
        scia: scia.clone(),
        righe_q: righe_q.clone(),
    };

    // La sessione di addestramento corrente (None finché non si preme "Impara")
    let sessione: Rc<RefCell<Option<Sessione>>> = Rc::new(RefCell::new(None));

    // --- Click sulle celle ---------------------------------------------------
    let weak = finestra.as_weak();
    let celle_cb = celle.clone();
    let sessione_cb = sessione.clone();
    let ui_cb = ui.clone();
    finestra.on_cella_cliccata(move |indice| {
        let finestra = weak.upgrade().unwrap();
        let modo = finestra.get_modo();
        let indice = indice as usize;

        // Nessun modo attivo: i click non fanno nulla
        if modo == 0 {
            return;
        }

        // Durante l'addestramento la griglia non si può modificare
        let in_corso = sessione_cb.borrow().as_ref().map_or(false, |s| !s.finito);
        if in_corso {
            finestra.set_info_tabella("Addestramento in corso: premi \"Reset\" per modificare la griglia.".into());
            return;
        }

        // Se si modifica la griglia, frecce, tabella Q e ricompense non valgono più
        if sessione_cb.borrow().is_some() {
            *sessione_cb.borrow_mut() = None;
            azzera_ui(&ui_cb);
        }

        let attuale = celle_cb.row_data(indice).unwrap();

        match modo {
            // Modo ostacoli: alterna X / vuota (solo su celle vuote o già con X)
            1 => match attuale {
                0 => celle_cb.set_row_data(indice, 1),
                1 => celle_cb.set_row_data(indice, 0),
                _ => {}
            },
            // Modo start (2) o target (3): un solo marcatore alla volta
            2 | 3 => {
                if attuale == modo {
                    // Click sul marcatore esistente: lo cancella
                    celle_cb.set_row_data(indice, 0);
                } else if attuale == 0 && !celle_cb.iter().any(|v| v == modo) {
                    // Cella vuota e marcatore non ancora presente: lo inserisce
                    celle_cb.set_row_data(indice, modo);
                }
            }
            _ => {}
        }
    });

    // --- Pulsante "Impara": legge la griglia e fa partire l'addestramento ------
    let celle_impara = celle.clone();
    let sessione_impara = sessione.clone();
    let ui_impara = ui.clone();
    finestra.on_impara(move || {
        match Ambiente::da_celle(&celle_impara, n as usize, m as usize) {
            Ok(env) => {
                let sess = Sessione::nuova(env);

                // Righe della tabella Q (una per cella libera e per lo start)
                let mm = sess.env.m;
                let righe: Vec<RigaQ> = sess
                    .mappa_righe
                    .iter()
                    .map(|&i| riga_q(&sess.env, &sess.q, (i / mm, i % mm), false))
                    .collect();
                ui_impara.righe_q.set_vec(righe);

                aggiorna_ui(&ui_impara, &sess);
                *sessione_impara.borrow_mut() = Some(sess);
            }
            Err(msg) => {
                if let Some(w) = ui_impara.finestra.upgrade() {
                    w.set_info(format!("Impossibile imparare: {}", msg).into());
                }
            }
        }
    });

    // --- Pausa / Riprendi ------------------------------------------------------
    let sessione_pausa = sessione.clone();
    let ui_pausa = ui.clone();
    finestra.on_pausa(move || {
        if let Some(sess) = sessione_pausa.borrow_mut().as_mut() {
            if !sess.finito {
                sess.in_pausa = !sess.in_pausa;
                aggiorna_ui(&ui_pausa, sess);
            }
        }
    });

    // --- Un passo alla volta (da usare in pausa) -----------------------------------
    let sessione_passo = sessione.clone();
    let ui_passo = ui.clone();
    finestra.on_passo(move || {
        if let Some(sess) = sessione_passo.borrow_mut().as_mut() {
            if sess.in_pausa && !sess.finito {
                sess.avanza();
                sess.riepilogo();
                if sess.finito {
                    sess.in_pausa = false;
                }
                aggiorna_ui(&ui_passo, sess);
            }
        }
    });

    // --- Reset: ferma tutto e cancella quanto appreso -------------------------------
    let sessione_reset = sessione.clone();
    let ui_reset = ui.clone();
    finestra.on_reset(move || {
        *sessione_reset.borrow_mut() = None;
        azzera_ui(&ui_reset);
    });

    // --- Pulsante "Analisi": grafico delle ricompense (finora) ----------------------
    let sessione_analisi = sessione.clone();
    let weak_analisi = finestra.as_weak();
    finestra.on_analisi(move || {
        let Some(w) = weak_analisi.upgrade() else {
            return;
        };
        let guardia = sessione_analisi.borrow();
        match guardia.as_ref() {
            Some(s) if !s.ricompense.is_empty() => {
                imposta_grafico(&w, &s.ricompense);
                w.set_overlay(2);
            }
            _ => w.set_info("Nessun dato da analizzare: premi prima \"Impara\".".into()),
        }
    });

    // --- Pulsante "Info Q": spiegazione dei valori Q ----------------------------------
    let weak_info = finestra.as_weak();
    finestra.on_info_q(move || {
        if let Some(w) = weak_info.upgrade() {
            imposta_info_q(&w);
            w.set_overlay(1);
        }
    });

    // --- Velocità e timer -------------------------------------------------------------------
    // Il timer fa avanzare l'addestramento: a velocità bassa un passo ogni tanto,
    // a velocità alta tanti passi per volta, al massimo a blocchi di BUDGET_MS.
    let timer = Rc::new(Timer::default());

    // (passi per scatto, modalità massima)
    let vel: Rc<Cell<(usize, bool)>> = Rc::new(Cell::new((1, false)));

    // Valore iniziale del cursore
    let velocita_iniziale = 50.0;
    finestra.set_velocita(velocita_iniziale as f32);
    let (ms0, passi0, max0, desc0) = parametri_velocita(velocita_iniziale);
    vel.set((passi0, max0));
    finestra.set_desc_velocita(desc0.into());

    // Quando l'utente sposta il cursore
    {
        let timer = timer.clone();
        let vel = vel.clone();
        let weak = finestra.as_weak();
        finestra.on_velocita_cambiata(move |v| {
            let (ms, passi, max, desc) = parametri_velocita(v as f64);
            vel.set((passi, max));
            timer.set_interval(Duration::from_millis(ms));
            if let Some(w) = weak.upgrade() {
                w.set_desc_velocita(desc.into());
            }
        });
    }

    // Lo scatto del timer
    {
        let sessione = sessione.clone();
        let vel = vel.clone();
        let ui = ui.clone();
        timer.start(TimerMode::Repeated, Duration::from_millis(ms0), move || {
            let mut guardia = sessione.borrow_mut();
            let Some(sess) = guardia.as_mut() else {
                return;
            };
            if sess.finito || sess.in_pausa {
                return;
            }

            let (passi, massima) = vel.get();
            if massima {
                // Alla massima velocità: lavora per BUDGET_MS e poi lascia
                // respirare la grafica (che si aggiorna una volta sola)
                let inizio = Instant::now();
                let mut contatore = 0u32;
                while !sess.finito {
                    sess.avanza();
                    contatore += 1;
                    if contatore % 64 == 0
                        && inizio.elapsed() >= Duration::from_millis(BUDGET_MS)
                    {
                        break;
                    }
                }
            } else {
                for _ in 0..passi {
                    if sess.finito {
                        break;
                    }
                    sess.avanza();
                }
            }

            sess.riepilogo();
            aggiorna_ui(&ui, sess);
        });
    }

    // Nel browser run() ritorna subito: tengo vivo il timer anche dopo la fine di questa funzione
    std::mem::forget(timer.clone());

    finestra.run()
}

// ---------------------------------------------------------------------------
// Punto di ingresso nel browser (WebAssembly)
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
mod web {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(start)]
    pub fn start() {
        // Mostra i panic nella console del browser invece di un generico "unreachable"
        console_error_panic_hook::set_once();
        let _ = super::avvia();
    }
}
