# Nachbesserungen (Runde 3): einheitliches Design

- **Adress-Komponente** spricht jetzt die Sprache des Besuchers (Beschriftungen, Platzhalter, Fehlermeldungen; `AddressLang`, Gästeseiten übergeben `lang.address()`), die Fehlertexte entstehen aus den Regeln statt aus englischen Systemmeldungen. Sie nutzt `Eyebrow`, `InfoTip` und `Notice`.
- **Neue zentrale Bausteine** in `ui/`: `Notice` (alle Meldungen), `Panel`, `Eyebrow`/`EYEBROW`, `Hint`, `InfoTip`, `Disclosure`/`DisclosureList`, `Checkbox` (boxed), `CheckboxCell`, `Switch`, `ChoiceSelect`, `PickSelect`, `FileInput`, `TextButton`, `ToggleButton`, `ToolButton`, `ChipButton`, `TileButton`, `FloatingButton`, `HeaderButton`, `CardButton`, `TagButton`, `Slider`, `ColorField`/`ColorInput`; `ActionMenu` kann Kopfzeile/eigenen Auslöser (Konto-Menü).
- **Umgestellt** (vorher eigene Klassen): 29 Hinweisboxen → `Notice`/`Panel`; drei verschiedene Beschriftungsstile → einer; 24 rohe Buttons, 13 Inputs, 6 Selects, 1 Textarea → Bausteine (Design-Editor, Cookie-Einstellungen und -Banner, Impressum, Dashboard-Import, Team-Liste, Plan-Seite, Header). Übrig bleiben nur dokumentierte Ausnahmen.
- **`src/ui_rules.rs`**: sechs Tests erzwingen das (keine rohen Controls, ein Beschriftungsstil, `Notice`/`Panel`, nur Palettenfarben, Überschriften aus der Typografie, keine Satz-Literale auf Gästeseiten).
- **Doku/Regeln**: `docs/DESIGN_GUIDE.md`, `AGENTS.md`, `tools/visual-check/` (Mock-API + Headless-Browser-Screenshots).

# Nachbesserungen (Runde 2)

- Standard der Share-Konfiguration: nur die E-Mail ist Pflicht, Ernährung optional. Einheitliche Fehleranzeige im Anmeldeformular (nichts ist rot, bevor es versucht wurde).
- Adresse: das leere Formular startete mit dem Platzhalter „[Address]“, der als eingegebene Adresse galt, deshalb waren nur Breiten-/Längengrad rot. Jetzt startet es leer, die Fehler werden bei jedem Versuch gezeigt, in beiden Eingabemodi mit einem gemeinsamen Hinweis.
- Ein Team mit unbestätigter E-Mail kann ändern und absagen (falsche Adresse korrigieren). Die geänderte Adresse setzt die Bestätigung zurück und löst eine neue Mail aus. „Bestätigen verlangen“ heißt jetzt nur noch: ohne Bestätigung kann das Team abgelehnt werden, und die Seite sagt das.
- Teamseite für das Handy neu: kompakter Kopf (Name, Status, zwei Chips, Fristzeile), Felder im Raster (Telefon neben Mitgliederzahl), Adresse als Zeile mit „ändern“, Link / Daten / Absage als aufklappbare Liste, feste Speichern-Leiste am unteren Rand. Die Bearbeitungsfrist hat immer eine eigene Zeile („möglich bis …“, „abgelaufen am …“ oder „jederzeit möglich“).
- Das klassische Blatt und die Vorlage „Classic“ sind entfernt; Gäste und Organisator sehen nur den Plan im gewählten Design, ohne Design eine Erklärung.

# Team-Ablauf über den Share-Link, Organisator-Werkzeuge, Konto-Menü (Punkte 1-17)

Gäste (`side/participant`)
- Einstellungen je Detail (Mail, Telefon, Mitglieder, Ernährung): **darf eingeben / Pflicht / löst Prüfung aus** (`FieldRules`, `ShareTeamConfig::{allows, requires, triggers_review}`). Formular, Anmeldung und Bearbeitung zeigen nur Erlaubtes, markieren Optionales und prüfen alles **vor** dem Senden (Pflichtfelder, Fristen, Mail-Bestätigung, Plan-Verfügbarkeit); nicht erlaubte Details werden nie gesendet und bleiben beim Bearbeiten unverändert.
- Anmeldung: `registration_full` aus der Konfiguration zeigt „Alle Plätze vergeben“ ohne Formular. Team-ID und Zugangstoken entstehen einmal im Browser; nach einem Verbindungsabbruch wird das Team über den Token gesucht (die Anmeldung kann trotzdem angekommen sein). Danach immer der persönliche Link mit Kopieren-Knopf, ohne Mailadresse mit deutlicher Warnung. Honeypot-Feld.
- Teamseite neu: oben eine **Statuskarte** (Teilnahme, E-Mail, Bearbeitung, Plan) mit den nötigen Hinweisen statt gestapelter Boxen; darunter Angaben links, Link / Daten / Absage rechts. Gesperrt (ausgegraut mit Begründung): abgesagt, Bearbeitungsfrist vorbei, geforderte Bestätigung fehlt. Absage nach der Frist nicht möglich, Hinweis auf den Veranstalter. Kein Löschen-Knopf mehr (nur der Organisator löscht).
- Bearbeiten als Gast sendet **keinen Status** mehr (Backend: `status` optional).
- E-Mail-Bestätigung: nur mit dem Secret des Mail-Links (`#token=…&verify=…`, wird beim Öffnen automatisch eingelöst). Der Link der Webseite bestätigt nichts. „Mail erneut senden“ zeigt die verbleibenden Versuche.
- Plan-Tab: ohne aktives Design (`plan_published = false`) kein Plan und kein Request, kein klassisches Blatt mehr.

Organisator
- Share-Konfiguration: Matrix „darf eingeben / Pflicht / Prüfung bei Änderung“ je Detail, hält die Regeln des Servers ein (Test mit 200 Klickfolgen).
- Team-Dialog: Block „Invitation email“ (bestätigt am … / nicht bestätigt, Einladung jederzeit erneut senden), abgesagte Teams komplett ausgegraut, „Absage bestätigen“ löscht das Team endgültig; Historie zeigt „Email address confirmed“.
- Calculation und Schedules zeigen einen Hinweis, solange kein Design für Teams aktiv ist; „Send route mails“ bleibt dann gesperrt.
- Header: ein Konto-Symbol mit kleinem Menü (Account & data, Profil bei Keycloak, Logout). Die Seite „Account & data“ zeigt das Profil und verlinkt auf die Keycloak-Kontoverwaltung.

Sonstiges: `MAX_MEMBERS` 255 (DB-Grenze), `CopyField` lokalisierbar, toter Ordner `side/details/share_team` entfernt.

Tests: `cargo test --bin tcc` grün, `cargo check --target wasm32-unknown-unknown` sauber. Nicht geprüft: Browser, Keycloak, Docker-Build.

# Anpassung an das produktionsreife Backend (Datenschutz, Links, Konfiguration, Auslieferung)

Links aus den Mails (`src/deeplink.rs`, `side/participant/team`, `side/admin_link.rs`)
- Team-Link wie vom Backend verschickt: `/project/{p}/team/{t}#token=…`. Das Token steht im URL-Fragment und landet damit in keinem Server- oder Proxy-Log. Der Start-Hash wird in `main()` vor dem Router gemerkt, das Token zusätzlich in `sessionStorage` des Tabs gehalten (Reload).
- Alte Pfad-Links (`/cook-and-run/{p}/team/{t}/{token}`, `/verify`) funktionieren weiter. `team_url()` (Kopieren des Teamlinks) erzeugt das neue Format.
- Organisator-Links `/admin/project/{p}` und `/admin/project/{p}/team/{t}` leiten weiter; der Team-Dialog öffnet sich direkt (`Teams { team }`).
- Bestätigung der Mailadresse: kein Auto-Verify beim Öffnen, sondern der bestehende Button „Jetzt bestätigen“.

Datenschutz (Backend: `docs/DATENSCHUTZ.md`)
- Teilnehmer: Karte „Deine Daten“ (Download der Team-Daten als JSON) und „Alle Daten löschen“ (`POST …/erase`, zweistufig bestätigt). Absage und Löschen sind im Text klar getrennt (Absage: für den Veranstalter sichtbar; Löschen: nichts bleibt).
- Organisator: neue Seite „Account & data“ (`/cook-and-run/account`): Export (`GET /me/export`), „Delete my account“ (`DELETE /me?confirm=true`, danach Keycloaks Aktion `delete_account`; optional mit den lokalen Browserdaten), reines Löschen der lokalen Daten (auch ohne Login). Nach der Rückkehr von Keycloak beendet der Callback den Ablauf, ohne einen Code einzulösen.
- Einwilligung nach Art. 9 DSGVO am Ernährungsfeld (Registrierung: Pflicht, sobald das Feld gefüllt ist; Team-Seite: bei neuem/geändertem Eintrag) und Datenschutzhinweis (Zweck, Empfänger, Fristen, Rechte) im Anmeldeformular.
- Datenschutzerklärung: neuer Abschnitt 2a (Teilnehmende, Art. 9, Fristen aus der Konfiguration), Datenzeile in der Tabelle.
- Projektübersicht: Hinweis, wann die Teilnehmerdaten gelöscht werden (Veranstaltungsdatum + konfigurierte Frist).

Routen-Mails (Backend: `POST …/plan/send-route-mails`)
- Plan-Editor, Menü „More“: „Send route mails…“ (`plan_editor/route_mails.rs`). Auswahl der Teams mit Mailadresse (abgesagte und Teams ohne Adresse werden nicht angeboten, weil das Backend sonst die ganze Anfrage ablehnt), Stapel zu 128, gesperrt bei veraltetem Plan (409) oder ungespeicherten Änderungen.

Konfiguration
- `config.json`: neuer Block `api.base_url` (die API-Adresse ist nicht mehr an `auth.audience` gekoppelt; ohne den Block gilt wie bisher die Audience) und `legal.retention_*_days` (nur Anzeige).
- Neue Auslieferung: `Dockerfile` (Tailwind → `dx bundle` → nginx-unprivileged, keine Umgebungsdaten im Image), `docker/nginx.conf.template` (SPA-Fallback, Cache-Regeln, Security-Header, CSP aus `API_ORIGIN`/`AUTH_ORIGIN`/`CSP_MODE`). `config.json` wird zur Laufzeit eingehängt.
- `tests/infra/`: alte Compose-Umgebung (`tcc:latest`) ersetzt durch `docker-compose.yaml`, das die Infrastruktur von `tcc-api/tests/infra` einbindet und `api` und `frontend` ergänzt; dazu `auth-proxy.conf`, `smoke-test.sh`, `check-urls.py`, `README.md` (Ablauf und Prüfliste).
- Das Realm `tcc-api/tests/infra/keycloak-realm.json` wurde angepasst (siehe README dort/unten).

Tests: `cargo test --lib` 108 grün (vorher 98), `cargo test --bin tcc` 68 grün, `cargo test --test config_files` 2 grün, `cargo check --target wasm32-unknown-unknown` sauber. Nicht geprüft werden konnten Browser, Keycloak und der Docker-Build (siehe `tests/infra/README.md`).

---

# Anpassung an den Frontend-Vertrag (Designs in der Cloud, Audit-Log)

Neu
- `src/design_wire.rs`: Wire-Format der Designs (`images[]` als `builtin`/`upload`), Vorab-Prüfung der Limits, 256-KiB-Grenze.
- `src/storage/design.rs`: Typen, Fehlerklassen (412/428/409/422/429 …), Blob-URL-Cache je `sha256`, ETag-Cache.
- `src/storage/cloud.rs`: Design-Endpunkte (Liste, POST, GET + ETag/If-None-Match, PATCH + If-Match, Löschen, aktives Design, Bild-Upload/-Abruf, Teilnehmer-Routen), Bilder zuerst und nacheinander, 409 bei Bild-ID -> neue UUID.
- `src/storage/local_designs.rs`: lokale Designs (localStorage) aus der UI in die Storage-Schicht verschoben; wandern beim Hoch-/Herunterladen eines Projekts mit.
- `src/storage/audit_text.rs` + `teams/history.rs`: Tab "History" im Team-Dialog (paginiert, [feld, neu, alt], Adresse nie roh).
- `schedules/cloud.rs`: Speichern auf Anforderung, "unsaved"-Erkennung, Konflikt-Dialog bei 412, "Use for teams".

Geändert
- `plan_config` entfernt (Storage, Cloud, Local, UI); Titel = Projektname, Datum = `occur`, Sprache wählt der Besucher.
- Platzhalter `{event_title}`, `{event_description}`, `{event_date}` entfernt; alte Designs werden beim Laden migriert.
- Farben klein geschrieben/normalisiert, Texte (`\r\n`, Steuer-/Bidi-Zeichen) und Namen bereinigt, doppelte IDs entfernt, GIF nicht mehr erlaubt.
- Teilnehmer: aktives Design mit ETag-Cache, 404 = klassisches Blatt, 409 = "Ihr Team hat abgesagt".

Tests: `cargo test --lib` (98 grün). `tests/` (Integrationstests) waren schon vorher nicht kompilierbar.

Fix: Gast-Anmeldung über die Share-URL
- `GET /project/{id}/share_team_config` schickte immer ein Bearer-Token mit; ohne Login brach der Client lokal mit "No auth data!" ab. Jetzt `Auth::BearerIfLoggedIn` (Token, falls eingeloggt, sonst anonym). Betrifft die Registrierungsseite und die Team-Seite der Gäste.
