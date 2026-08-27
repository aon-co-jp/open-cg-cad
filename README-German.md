# open-cg-cad

*日本語*: [README.md](README.md) · *English*: [README-English.md](README-English.md)

Ein CAD-System für "KI-Baufirma & KI-Gebäude"-Funktionen. Es prüft
Bauvorschriften (Sonnenlichtrecht, Bebauungsdichte, Geschossflächenzahl
usw.), erzeugt automatisch 3D-Gebäudevolumen als CAD-Daten, wendet einfache
Einfärbung an und kann mehrere Baumethoden vorschlagen.

**Hinzugefügt am 2026-08-27**: Eine Redesign-Funktion für alte Zeichnungen
von Halbleitern (CPU/NPU/GPU) und Fahrzeugen (Auto/Motorrad/Shinkansen/
Transrapid/Flugzeug, einschließlich Benzin-/Diesel-/EV-/PHEV-Motoren) —
der Textbeschreibung einer alten Zeichnung wird eine neue Anweisung oder
ein erneut hochgeladenes Spezifikationsdokument gegenübergestellt, und die
KI erstellt einen Redesign-Vorschlag — sowie GitHub- und VPS/Mietserver-
Lese-/Schreib-Agenten. Siehe [CLAUDE.md](CLAUDE.md) (Japanisch) für den
vollständigen Umfang und die ehrliche Offenlegung dessen, was implementiert
ist (und was nicht) — insbesondere: es werden keine physikalischen,
elektrischen oder aerodynamischen Simulationen durchgeführt und keine
tatsächlichen CAD-Geometriedaten verändert; es handelt sich ausschließlich
um textbasierte KI-Beratung.

`https://easy-web.tokyo/open-cg-cad/` ist live im Einsatz; ein Demo- und
Download-Einstiegspunkt (für den lokalen Betrieb auf dem eigenen Laufwerk)
befindet sich unter `https://easy-web.tokyo/open-cg-cad/demo`.

**Wichtiger Haftungsausschluss**: Dieses System ist lediglich ein
Entwurfs-/Beispielgenerierungstool, das die abschließende Prüfung durch
qualifizierte Fachleute (Architekten usw.) vorbereitet — es erzeugt keine
rechtsverbindlichen oder baugenehmigungsfähigen Pläne.
