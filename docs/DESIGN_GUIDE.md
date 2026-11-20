# Design-Guide (tcc-client)

Dieser Guide ist die Quelle für jede Oberflächenänderung. Er wird durch `src/ui_rules.rs` (Tests) durchgesetzt
und durch `AGENTS.md` als Regel für Mitwirkende und KI-Assistenten referenziert.

## 1. Grundsatz

**Eine Oberfläche besteht aus Bausteinen von `crate::ui`.** Eine Seite baut keine eigenen Buttons, Felder,
Hinweisboxen, Überschriften oder Beschriftungsstile. Fehlt ein Baustein, wird er in `ui/` ergänzt (mit Beispiel in
diesem Guide) und dann benutzt, nicht lokal nachgebaut. Beim zweiten Mal Kopieren ist es ein Baustein.

## 2. Gestaltungswerte

| Thema | Wert |
|---|---|
| Palette | Marke Amber (`amber-*`), Text und Flächen Zinc (`zinc-*`), Zustände Green / Red / Emerald. **Keine** anderen Farbfamilien (Test) |
| Text | Fließtext `text-sm`, Nebentext `text-xs`, Titel `Headline1..3`; Beschriftungen immer `Eyebrow` / `EYEBROW` |
| Rundung | Eingaben, Buttons, Hinweise `rounded-xl`; Karten `rounded-2xl`; kleine Schalter `rounded-lg` |
| Fokus | sichtbarer Ring (`focus-visible:ring-2 ring-amber-400`), steckt in den Bausteinen |
| Trefferfläche | mindestens ca. 44 px Höhe für alles, was auf dem Handy getippt wird |
| Kontrast | Text ≥ 4,5:1 (WCAG 2.1 AA); Information nie nur durch Farbe, immer auch Text oder Symbol |
| Handy zuerst | Seiten sind für 390 px entworfen und werden darüber erweitert (`sm:`, `lg:`). Auf dem Handy: Karten randlos (`max-sm:rounded-none max-sm:border-x-0`), Aktionen in Reichweite des Daumens (feste Leiste unten), Seltenes eingeklappt |

## 3. Bausteine und wann man sie nimmt

| Bedarf | Baustein |
|---|---|
| Hauptaktion / Nebenaktion / Gefahr | `Button` (`Primary`, `Secondary`, `Danger`, `DangerOutline`) |
| Link, der wie Button aussieht | `Link { class: button_class(..) }` |
| Aktion im Fließtext („Alle auswählen“) | `TextButton` |
| An/aus-Auswahl (Stil, Variante) | `ToggleButton` (mit `aria-pressed`), Werkzeugleiste: `ToolButton` |
| Einzige Hauptaktion einer Seite, schwebend | `FloatingButton` |
| Ganze Karte klickbar | `CardButton`; farbig nach Daten: `TagButton` |
| Kopfzeile der App | `HeaderButton`, Menü: `ActionMenu { header: true }` |
| Text / Zahl / Mehrzeilig | `Field { TextInput }`, `TextArea` |
| Auswahl bekannter Werte / Laufzeitwerte | `Select`, `ChoiceSelect`; „wähle, um zu handeln“: `PickSelect` |
| Ja/Nein im Formular | `Checkbox` (`boxed` für Einwilligungen), in Tabellen `CheckboxCell`; sofort wirksam: `Switch` |
| Datei wählen | `label { for }` im Button-Look + `FileInput` |
| Farbe, Regler (Design-Werkzeuge) | `ColorField`, `ColorInput`, `Slider` |
| Meldung in der Seite (Hinweis, Warnung, Fehler, Erfolg) | **`Notice`** mit `NoticeTone`; Streifen in Karten: `flat` |
| Kleine Beschriftung | `Eyebrow` / `EYEBROW`; Erklärtext unter Feldern: `Hint` |
| Erklärung per Hover/Fokus | `InfoTip` |
| Gruppe zusammengehöriger Einstellungen | `Panel` (`flush`, wenn der Inhalt eigenes Padding hat) |
| Selten Gebrauchtes einklappen | `DisclosureList` + `Disclosure` (`danger` für Löschen/Absagen) |
| Zustand als Marke | `Pill` (`PillTone`); Tabs: `Tabs`, `SegmentedTabs` (Sprachumschalter) |
| Leerer Zustand | `EmptyState`; Laden/Fehler: `render_resource`, `LoadingView`, `ErrorView` |
| Kopieren eines Werts | `CopyField` (mit lokalisierten Beschriftungen) |

## 4. Muster

**Formulare.** Pflichtfelder sind unmarkiert, Optionales trägt „(optional)“. Fehler erscheinen **erst nach dem ersten
Versuch** (oder wenn ein ausgefülltes Feld ungültig ist), einheitlich für alle Felder, nie vorab für einzelne. Beim
Absenden werden *alle* Fehler sichtbar gemacht, auch die der Adresse. Was der Server ablehnen würde und die Seite selbst
prüfen kann (Pflicht, Fristen, Sperren, Limits), prüft die Seite vor dem Senden.

**Gesperrte Bereiche.** Nicht änderbar = ausgegraut (`opacity-60 pointer-events-none`), ein `Notice` sagt in einem Satz
warum. Die Begründung steht einmal, nicht dreimal.

**Leere Startwerte.** Ein neues Formular beginnt leer (`AddressData::empty()`), nie mit Platzhalterdaten, die wie
Eingaben gelten.

**Mehrsprachigkeit.** Alles, was Gäste lesen, kommt aus `side/participant/lang.rs` (Deutsch und Englisch, ein Feld je
Satz). Komponenten, die auf Gästeseiten stehen (z. B. `Address`), nehmen eine Sprache entgegen und bringen beide
Texte mit. Die Seiten der Organisatoren sind englisch.

**Handy-Layout einer Seite.** Kopf kompakt (Name, Zustand, Chips, eine Fristzeile) → Hauptinhalt → Seltenes eingeklappt.
Kurze Felder nebeneinander (Telefon + Anzahl), Adresse als Zeile mit „ändern“, Speichern als feste Leiste unten, sobald
etwas geändert ist.

**Zugänglichkeit.** Jedes Bedienelement hat einen zugänglichen Namen; Fehler `role="alert"`, Hinweise `role="status"`
(steckt in `Notice`); Schalter `aria-pressed` / `role="switch"`; Tabellen mit `th scope`; Fokus nie verstecken.

## 5. Neuen Baustein anlegen

1. In `ui/<datei>.rs` anlegen, Doc-Kommentar mit *wann man ihn nimmt*.
2. In `ui/mod.rs` exportieren und in die Tabelle oben eintragen.
3. Bestehende Stellen umstellen (Suche nach dem alten Muster), `cargo test --bin tcc` muss grün bleiben.
4. Ausnahmen in `src/ui_rules.rs` nur mit Kommentar *warum*; die Listen sollen schrumpfen.

## 6. Prüfen

- `cargo test --bin tcc`: enthält die Design-Regeln (keine rohen Buttons/Felder, ein Beschriftungsstil, `Notice`/
  `Panel` statt eigener Boxen, nur Palettenfarben, Überschriften aus der Typografie, keine Satz-Literale auf Gästeseiten).
- Sichtprüfung im Browser bei 390 px und 1200 px: `tools/visual-check/README.md` (WASM bauen, Mock-API, Screenshots).
  Vor jeder Auslieferung mindestens die geänderten Seiten in den Zuständen normal, leer/Fehler, gesperrt ansehen.
- Neue Tailwind-Klassen: `assets/output.css` neu erzeugen (siehe `Dockerfile`).
