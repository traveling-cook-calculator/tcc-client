# Fonts of the schedule designer

The fonts are part of the project so that no font server is contacted and the schedule sheets can
be exported as images reliably. Source: the Fontsource npm packages (Latin subset, WOFF2).

| Font | Used for | License |
|---|---|---|
| Chewy | Titles of the classic sheet | Apache 2.0 (`LICENSE-Chewy-Apache-2.0.txt`) |
| Gluten | Body text of the classic sheet | SIL Open Font License 1.1 |
| Archivo Black | Heavy titles | SIL Open Font License 1.1 |
| Bebas Neue | Condensed numbers and headings | SIL Open Font License 1.1 |
| Inter | Clean body text | SIL Open Font License 1.1 |
| Pacifico | Script accents | SIL Open Font License 1.1 |
| Playfair Display | Serif titles | SIL Open Font License 1.1 |

To add a font: put the `.woff2` here, add an `@font-face` in `assets/input.css` and an entry in
`Font` in `src/schedule_design.rs`.
