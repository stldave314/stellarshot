# Application
stellarshot = Stellarshot

# Empty state
empty-title = Halte deine Dateien sicher
empty-body = Sichere deine Ordner auf ein anderes Laufwerk oder in einen anderen Ordner. Sicherungen sind verschlüsselt, und nach der ersten werden nur Änderungen gespeichert.
create-backup = Sicherung erstellen …
open-existing = Vorhandene Sicherung öffnen

# Profile page
never-backed-up = Noch nicht gesichert
backed-up-just-now = Gerade eben gesichert
backed-up-minutes-ago = Letzte Sicherung vor { $count ->
    [one] einer Minute
   *[other] { $count } Minuten
}
backed-up-hours-ago = Letzte Sicherung vor { $count ->
    [one] einer Stunde
   *[other] { $count } Stunden
}
backed-up-days-ago = Letzte Sicherung vor { $count ->
    [one] einem Tag
   *[other] { $count } Tagen
}
status-detail = { $destination } · { $count ->
    [one] eine Momentaufnahme
   *[other] { $count } Momentaufnahmen
}
back-up-now = Jetzt sichern
choose-what-title = Wähle, was gesichert wird
choose-what-body = Für diese Sicherung sind noch keine Ordner ausgewählt.
choose-what-button = Ordner wählen …
unlock-title = Gib das Passwort dieser Sicherung ein
unlock = Entsperren
remember-password = Passwort merken
remember-password-description = Wird in deinem Schlüsselbund gespeichert. Geplante Sicherungen brauchen es.
recent-snapshots = Letzte Momentaufnahmen
no-snapshots-yet = Noch keine Momentaufnahmen. Drücke „Jetzt sichern“ für die erste.
snapshot-row = { $id } · { $size } · { $added } neu
show-all-snapshots = Alle { $count } Momentaufnahmen anzeigen
manage = Verwalten
edit-backup = Was gesichert wird
edit-backup-description = Ein- und ausgeschlossene Ordner ändern.
change-password-row = Passwort
change-password-row-description = Ändert das Passwort, mit dem diese Sicherung entsperrt ist.
change-password-title = Passwort ändern
change-password-body = Fügt einen Schlüssel für das neue Passwort hinzu und entfernt dann den, mit dem du gerade entsperrt bist. Der Schlüssel einer anderen Person oder eines Wiederherstellungsblatts bleibt unberührt.
change-password-command-note = Du musst es außerdem überall dort aktualisieren, wo das aktuelle Passwort gespeichert ist — dies ändert nur die Sicherung selbst.
change-password-failed = Das Passwort konnte nicht geändert werden.
password-source-row = Woher das Passwort kommt
password-source-keyring = Im Schlüsselbund gespeichert
password-source-command = Von einem Befehl
password-source-title = Woher das Passwort kommt
password-source-body = Ein Befehl, der das Passwort auf seiner Standardausgabe ausgibt, jedes Mal frisch ausgeführt, wenn eines gebraucht wird, anstelle des Schlüsselbunds — für einen Passwortmanager mit Kommandozeilen-Client, etwa die Bitwarden-CLI. Leer lassen, um den Schlüsselbund zu verwenden.
password-source-placeholder = z. B. bw get password stellarshot-home
password-command-failed = Der Passwortbefehl konnte nicht ausgeführt werden.
remove-backup = Aus Stellarshot entfernen
remove-backup-description = Stellarshot vergisst diese Sicherung. Die Sicherung selbst bleibt, wo sie ist.
delete-backup = Sicherung und alle Daten löschen
delete-backup-description = Löscht jede Momentaufnahme dieser Sicherung endgültig.
progress-starting = Wird gestartet …
progress-preparing = Wird vorbereitet …
progress-backing-up = Wird gesichert …
progress-restoring = Wird wiederhergestellt …
progress-checking = Wird geprüft …
progress-amount = { $done } von { $total }

# Wizard
wizard-create-title = Neue Sicherung
wizard-open-title = Vorhandene Sicherung öffnen
wizard-edit-title = Was „{ $name }“ sichert
wizard-step = Schritt { $current } von { $total }
wizard-what-intro = Wähle die zu sichernden Ordner und was darin ausgelassen werden soll.
wizard-include = Einschließen
wizard-exclude = Ausschließen
wizard-exclude-outside = Liegt in keinem eingeschlossenen Ordner und ändert daher nichts
wizard-add-folders = Ordner hinzufügen …
browse-open = Durchsuchen …
browse-close = Schließen
browse-scanning = Wird gescannt … bisher { $count } gefunden
browse-mark-partial = Teilweise eingeschlossen
browse-size-reduced = { $size } von { $total }
wizard-pick-sources = Zu sichernde Ordner wählen
wizard-pick-excludes = Auszulassende Ordner wählen
wizard-advanced = Erweitert
wizard-pattern-placeholder = Passende Namen auslassen, z. B. *.tmp oder node_modules
wizard-one-file-system = Auf demselben Laufwerk bleiben
wizard-one-file-system-description = Keinen anderen Laufwerken oder Netzwerkfreigaben folgen, die in diesen Ordnern eingehängt sind.
wizard-estimate-label = Geschätzte Größe der Sicherung
wizard-estimate = { $size } · { $files } Dateien
wizard-estimate-counting = Wird gezählt …
wizard-estimate-note = Die erste Sicherung ist nach Kompression und Deduplizierung meist kleiner. Spätere Sicherungen speichern nur Änderungen.
wizard-where-intro = Wähle, wo die Sicherung aufbewahrt wird: ein leerer Ordner, am besten auf einem anderen Laufwerk.
wizard-where-title = Speicherort
wizard-no-folder = Kein Ordner gewählt
wizard-choose-folder = Ordner wählen …
select-repo-folder = Wähle einen Ordner für das Archiv
wizard-where-new = Hier wird eine neue Sicherung erstellt.
wizard-where-existing = Dieser Ordner enthält bereits eine Sicherung. Füge sie stattdessen über „Vorhandene Sicherung öffnen“ hinzu.
wizard-where-found = Hier wurde eine Sicherung gefunden.
wizard-where-no-repository = In diesem Ordner gibt es keine Sicherung.
wizard-where-not-empty = Dieser Ordner enthält bereits andere Dateien. Wähle einen leeren Ordner.
wizard-name = Name
wizard-name-placeholder = Zum Beispiel: Persönlicher Ordner auf USB-Laufwerk
wizard-secure-intro = Wähle ein Passwort. Deine Sicherung wird damit verschlüsselt.
wizard-open-intro = Gib das Passwort ein, mit dem diese Sicherung erstellt wurde.
wizard-confirm = Passwort bestätigen
wizard-mismatch = Die Passwörter stimmen nicht überein.
wizard-password-warning = Wenn du dieses Passwort verlierst, können deine Sicherungen nicht wiederhergestellt werden. Niemand kann es für dich wiederherstellen.
wizard-finish-create = Erstellen und jetzt sichern
wizard-finish-open = Öffnen

# Dialogs and buttons
ok = OK
save = Speichern
add = Hinzufügen
back = Zurück
next = Weiter
edit = Bearbeiten
remove = Entfernen
delete = Löschen
cancel = Abbrechen
password = Passwort
remove-title = „{ $name }“ entfernen?
remove-body = Stellarshot vergisst diese Sicherung und ihr gespeichertes Passwort. Die Sicherung und ihre Momentaufnahmen werden nicht gelöscht und können später wieder geöffnet werden.
delete-title = „{ $name }“ und alle Daten löschen?
delete-body = Dadurch werden die Sicherung und jede Momentaufnahme darin endgültig gelöscht. Andere Dateien im selben Ordner bleiben unberührt. Gib { $name } zur Bestätigung ein.

# Errors
error-title = Etwas ist schiefgelaufen
error-details = Details: { $details }
location-not-empty = { $path } enthält bereits andere Dateien. Wähle einen leeren Ordner oder einen Ordner, der bereits ein Archiv enthält.
create-repo-failed = Das Archiv konnte nicht erstellt werden.
delete-repo-failed = Das Archiv konnte nicht gelöscht werden.
delete-snapshot-failed = Die Momentaufnahme konnte nicht gelöscht werden.
snapshot-failed = Die Momentaufnahme konnte nicht erstellt werden.
open-repo-failed = Das Archiv konnte nicht geöffnet werden.
error-wrong-password = Das Passwort ist falsch.
error-not-a-repository = Unter { $path } gibt es kein Archiv.
error-already-exists = { $path } enthält bereits ein Archiv.
error-destination-unavailable = { $path } ist nicht erreichbar. Wenn es sich auf einem Wechseldatenträger oder einer Netzwerkfreigabe befindet, prüfe, ob es verbunden ist.
error-locked = Eine andere Sicherung verwendet dieses Archiv bereits. Versuche es erneut, wenn sie abgeschlossen ist.
error-canceled = Der Vorgang wurde abgebrochen. Es wurde nichts verändert.
error-repository-damaged = Die Prüfung des Archivs hat Probleme gefunden. Lösche keine anderen Kopien deiner Daten, bis das behoben ist.

# About
about = Über
about-author = Die Stellarshot-Mitwirkenden
about-credits = Basiert auf Stellarshot vom Projekt cosmic-utils.
repository = Quellcode
support = Hilfe

# Settings
settings = Einstellungen
appearance = Aussehen
theme = Färbung
match-desktop = An den Desktop angeglichen
dark = Dunkel
light = Hell

# Menu
file = Datei
menu-new-backup = Neue Sicherung …
new-backup = Neue Sicherung
new-window = Neues Fenster
quit = Verlassen
view = Ansicht
menu-settings = Einstellungen
menu-about = Informationen über Stellarshot

# Storage locations
google-drive = Google Drive
place-folder = Ordner
place-folder-description = Ein Ordner auf diesem Rechner oder einem eingehängten Laufwerk
place-drive = Wechseldatenträger
place-drive-description = Ein USB-Laufwerk, egal wo es eingehängt ist
place-server = Netzwerkserver (SFTP)
place-server-description = Ein Ordner auf einem Rechner, den du per SSH erreichst
place-google-description = Dein Google-Konto, angemeldet in Stellarshot
place-remote = Eines deiner rclone-Remotes
place-remote-description = OneDrive, Dropbox, S3 und alles andere, was rclone erreicht
place-rest = REST-Server
place-rest-description = Ein rest-server oder rustic-server, den du selbst betreibst
place-rest-url = Server-URL
place-rest-url-placeholder = http://user:pass@host:8000/repo/
place-rclone-missing = Um hier zu sichern, wird rclone benötigt. Installiere es (zum Beispiel mit sudo apt install rclone) und versuche es erneut.
place-check-failed = Dieser Speicherort konnte nicht geprüft werden.
place-check = Prüfen
place-no-drives = Es sind keine Wechseldatenträger angeschlossen. Schließe einen an und gehe dann zurück und wieder weiter.
place-folder-on-drive = Ordner auf dem Laufwerk
place-host = Server
place-user = Benutzername
place-user-placeholder = Dein Benutzername auf diesem Rechner
place-port = Port
place-server-path = Ordner auf dem Server
place-server-note = Die Anmeldung erfolgt über deinen SSH-Agenten oder deine Schlüssel, und der Server muss bereits in ~/.ssh/known_hosts stehen: Verbinde dich zuerst einmal per ssh.
place-signing-in = Schließe die Anmeldung im Browser ab. Stellarshot wartet …
place-signing-in-title = Mit Google anmelden
place-signing-in-body = Stellarshot hat deinen Standardbrowser geöffnet, um die Anmeldung abzuschließen. Wechsle dorthin, um fortzufahren, und komm dann hierher zurück.
place-google-intro = Stellarshot öffnet deinen Browser, damit du dich bei Google anmelden kannst. Die Anmeldung wird nur in Stellarshots eigenen Einstellungen gespeichert.
place-sign-in = Mit Google anmelden …
place-signed-in = Angemeldet.
place-cloud-folder = Ordner
place-no-remotes = Du hast keine rclone-Remotes. Richte eines mit rclone config ein und komm dann zurück.
error-rclone-missing = rclone ist nicht installiert. Installiere es (zum Beispiel mit sudo apt install rclone), um diesen Speicherort zu nutzen.
error-auth-failed = Die Anmeldung wurde nicht abgeschlossen: { $details }

# Déjà Dup import
dejadup-import = Aus Déjà Dup importieren
dejadup-title = Eine Déjà-Dup-Sicherung importieren
dejadup-name = Déjà-Dup-Sicherung
dejadup-none = Es wurden keine Déjà-Dup-Einstellungen gefunden.
dejadup-other-format = Diese Déjà-Dup-Sicherung verwendet das ältere duplicity-Format, das Stellarshot nicht lesen kann. Erstelle stattdessen eine neue Sicherung; die alte bleibt in Déjà Dup lesbar.
dejadup-unsupported = Déjà Dup bewahrt diese Sicherung an einem Ort auf, den Stellarshot nicht nutzen kann ({ $backend }). Erstelle stattdessen eine neue Sicherung.
menu-import-dejadup = Aus Déjà Dup importieren …

# Restore
restore-open = Wiederherstellen …
restore-title = Wiederherstellen aus { $name }
restore-loading = Sicherung wird geöffnet …
folder-up = Einen Ordner nach oben
tab-browse = Durchsuchen
tab-deleted = Gelöschte Dateien
tab-compare = Vergleichen
selected-count = { $count ->
    [one] 1 Element ausgewählt
   *[other] { $count } Elemente ausgewählt
}
restore-button = Wiederherstellen …
search-placeholder = Diese Momentaufnahme durchsuchen
search-results = { $count ->
    [one] 1 Treffer
   *[other] { $count } Treffer
}
folder-empty = Dieser Ordner ist leer.
versions-title = Versionen
versions-same = { $count ->
    [one] ↳ in 1 älteren Momentaufnahme gleich
   *[other] ↳ in { $count } älteren Momentaufnahmen gleich
}
open-copy = Kopie öffnen
download = Herunterladen …
restore-mount = Als Ordner einbinden …
restore-this-version = Diese Version wiederherstellen …
deleted-scope = Dateien in { $folder }, die in Sicherungen der letzten { $days } Tage enthalten, aber nicht mehr vorhanden sind.
deleted-change-folder = Ordner ändern …
deleted-find = Gelöschte Dateien suchen
restore-searching = Wird gesucht …
deleted-intro = Suche nach gelöschten Dateien, die eine Sicherung noch enthält.
deleted-none = Nichts fehlt: Jede Datei aus diesen Sicherungen ist noch vorhanden.
deleted-last-seen = zuletzt gesichert { $when }
compare-button = Vergleichen
compare-intro = Wähle zwei Momentaufnahmen, um zu sehen, was sich geändert hat.
compare-none = Zwischen diesen Momentaufnahmen hat sich nichts geändert.
compare-summary = { $added } hinzugefügt · { $removed } entfernt · { $changed } geändert
restore-sheet-title = { $count ->
    [one] 1 Element wiederherstellen
   *[other] { $count } Elemente wiederherstellen
}
restore-to = Wiederherstellen nach
restore-to-original = An den ursprünglichen Ort
restore-to-folder = In einen anderen Ordner …
restore-to-folder-chosen = In { $folder }
restore-existing = Wenn eine Datei bereits existiert
policy-keep-both = Beide behalten
policy-keep-both-description = Die wiederhergestellte Kopie erhält einen neuen Namen; deine Datei bleibt unberührt.
policy-overwrite = Überschreiben
policy-overwrite-description = Durch die gesicherte Kopie ersetzen.
policy-skip = Überspringen
policy-skip-description = Behalten und diese Datei nicht wiederherstellen.
restore-previewing = Es wird ermittelt, was passieren wird …
restore-choose-folder = Wähle den Ordner, in den wiederhergestellt werden soll.
restore-preview-failed = Es konnte nicht ermittelt werden, was die Wiederherstellung tun würde.
preview-restore = { $count ->
    [one] 1 Datei wird wiederhergestellt ({ $size })
   *[other] { $count } Dateien werden wiederhergestellt ({ $size })
}
preview-kept = { $count ->
    [one] 1 vorhandene Datei weicht ab und wird daneben behalten
   *[other] { $count } vorhandene Dateien weichen ab und werden daneben behalten
}
preview-replaced = { $count ->
    [one] 1 vorhandene Datei weicht ab und wird ersetzt
   *[other] { $count } vorhandene Dateien weichen ab und werden ersetzt
}
preview-skipped = { $count ->
    [one] 1 vorhandene Datei weicht ab und wird übersprungen
   *[other] { $count } vorhandene Dateien weichen ab und werden übersprungen
}
preview-unchanged = { $count ->
    [one] 1 Datei ist bereits identisch und bleibt unberührt
   *[other] { $count } Dateien sind bereits identisch und bleiben unberührt
}
restore-done-title = Wiederherstellung abgeschlossen
restore-done-body = { $count ->
    [one] 1 Datei wiederhergestellt ({ $size }).
   *[other] { $count } Dateien wiederhergestellt ({ $size }).
} { $conflicts ->
    [0] {""}
   *[other] Bereits vorhandene Dateien wurden wie gewählt behandelt.
}
restore-failed = Die Wiederherstellung wurde nicht abgeschlossen.
browse-failed = Diese Sicherung konnte nicht gelesen werden.
open-copy-failed = Es konnte keine Kopie dieser Datei geöffnet werden.
download-title = Kopie aus dieser Sicherung speichern
download-failed = Es konnte keine Kopie aus dieser Sicherung gespeichert werden.
select-scope-folder = Wähle einen Ordner zum Durchsuchen
select-restore-folder = Wähle, wohin wiederhergestellt werden soll
select-mount-folder = Wähle einen leeren Ordner zum Einbinden
mount-active = Eingebunden unter { $folder }
mount-open-folder = Ordner öffnen
unmount = Aushängen
mount-failed = Diese Sicherung konnte nicht als Ordner eingebunden werden.

# Automation
error-password-not-remembered = Geplante Sicherungen benötigen das im Schlüsselbund gespeicherte Passwort. Öffne die Sicherung, gib ihr Passwort mit „Passwort merken“ ein, dann läuft die nächste geplante Sicherung.
error-keyring-unavailable = Das Passwort wurde geändert, konnte aber nicht im Schlüsselbund gespeichert werden: { $details }. Trage es dort selbst ein, sonst schlägt eine geplante Sicherung damit fehl.
error-delete-unsupported = Stellarshot kann die Daten dieses Ziels nicht selbst löschen. Entferne sie dort selbst, oder verwende „Entfernen“, um sie hier zu vergessen, ohne etwas zu löschen.
change = Ändern …
schedule-row = Wann sie läuft
hooks-row = Hooks
hooks-row-none = Keine Hooks eingerichtet
hooks-row-count = { $count ->
    [one] 1 Hook aktiviert
   *[other] { $count } Hooks aktiviert
}
check-row = Auf Schäden prüfen
check-row-last = Zuletzt geprüft { $when }
check-row-never = Noch nie geprüft
check-now = Jetzt prüfen
check-again = Erneut prüfen
check-failed = Die Prüfung wurde nicht abgeschlossen.
check-passed-title = Keine Schäden gefunden
check-passed-body = Jede Momentaufnahme, jeder Ordner und jeder Indexeintrag dieser Sicherung ist vorhanden und stimmig.
clean-up-row = Speicher freigeben
clean-up-row-description = Vergisst Momentaufnahmen, die die Einstellung „Behalten“ nicht mehr braucht, und löscht Daten, die keine Momentaufnahme nutzt.
clean-up-now = Jetzt aufräumen
clean-up-failed = Das Aufräumen wurde nicht abgeschlossen.
clean-up-cannot-stop = Das Freigeben von Speicher kann nach dem Start nicht angehalten werden.
clean-up-done-title = Aufräumen abgeschlossen
clean-up-done-body = { $count ->
    [one] 1 Momentaufnahme vergessen.
   *[other] { $count } Momentaufnahmen vergessen.
} { $size } werden nicht mehr benötigt.
progress-cleaning-up = Speicher wird freigegeben …
damaged-title = Eine Prüfung hat Schäden in dieser Sicherung gefunden
damaged-body = Das automatische Freigeben von Speicher ist angehalten, bis eine Prüfung besteht. Momentaufnahmen lassen sich eventuell noch wiederherstellen; sicherheitshalber beginne eine neue Sicherung an einem anderen Ort.
failed-just-now = gerade eben
failed-minutes-ago = { $count ->
    [one] vor einer Minute
   *[other] vor { $count } Minuten
}
failed-hours-ago = { $count ->
    [one] vor einer Stunde
   *[other] vor { $count } Stunden
}
failed-days-ago = { $count ->
    [one] gestern
   *[other] vor { $count } Tagen
}
scheduled-backup-failed = Die automatische Sicherung ist fehlgeschlagen ({ $when })
scheduled-cleanup-failed = Das Aufräumen nach der automatischen Sicherung ist fehlgeschlagen ({ $when })
scheduled-check-failed = Die automatische Prüfung ist fehlgeschlagen ({ $when })
schedule-failed = Der Zeitplan konnte nicht eingerichtet werden.
schedule-manual = Sichert nur, wenn du „Jetzt sichern“ drückst
schedule-hourly = Sichert automatisch jede Stunde
schedule-daily = Sichert automatisch jeden Tag
schedule-weekly = Sichert automatisch jede Woche
schedule-on-connect = Sichert automatisch, sobald das Laufwerk angeschlossen wird
frequency-hourly = Jede Stunde
frequency-daily = Jeden Tag
frequency-weekly = Jede Woche
frequency-on-connect = Sobald das Laufwerk angeschlossen wird
keep-smart = Intelligent (empfohlen)
keep-3-months = Mindestens 3 Monate
keep-6-months = Mindestens 6 Monate
keep-1-year = Mindestens ein Jahr
keep-days = Mindestens { $days } Tage
keep-forever = Für immer
keep-smart-description = Behält die neueste Momentaufnahme von jedem der letzten 7 Tage, an denen es eine gibt, von jeder der letzten 4 Wochen (Montag bis Sonntag), in denen es eine gibt, und von jedem der letzten 12 Kalendermonate, in denen es eine gibt. Tage, Wochen und Monate ohne Sicherung werden übersprungen, nicht mitgezählt, und eine Momentaufnahme kann zugleich die für ihren Tag, ihre Woche und ihren Monat sein. Solange die Sicherungen weniger als 12 Monate umfassen, wird auch die allererste Momentaufnahme behalten. Jede andere Momentaufnahme wird vergessen. Betroffen sind nur die Momentaufnahmen dieses Computers, und jede gesicherte Ordnerauswahl wird für sich gezählt.
keep-forever-description = Jede Momentaufnahme wird behalten. Die Sicherung wächst nur.
keep-for-description = Jede Momentaufnahme aus den { $days } Tagen vor der neuesten, ältere werden nach und nach entfernt.
wizard-schedule-title = Wann „{ $name }“ läuft
wizard-when-intro = Sicherungen können von selbst laufen. Ist der Computer zu der Zeit aus oder im Ruhezustand, läuft die Sicherung, sobald du zurück bist.
wizard-automatic = Automatisch sichern
wizard-automatic-description = Läuft im Hintergrund, auch wenn Stellarshot geschlossen ist.
wizard-frequency = Wie oft
wizard-keep = Alte Momentaufnahmen
wizard-keep-label = Behalten
wizard-prune = Speicher automatisch freigeben
wizard-prune-description = Löscht Daten, die keine Momentaufnahme mehr braucht. Lass das aus, wenn ein anderer Computer an denselben Ort sichert.
wizard-compression = Kompression
wizard-compression-description = Wie stark jede Sicherung komprimiert wird: langsamer und kleiner oder schneller und größer. Wird nur beim Erstellen einer Sicherung festgelegt; rustics eigene Werkzeuge können es später noch ändern.
wizard-compression-default = Standard
wizard-compression-fast = Schnell
wizard-compression-best = Beste
wizard-append-only = Nur anhängen
wizard-append-only-description = Ein Schutz gegen Fehler, nicht gegen Angriffe: rustic selbst weigert sich, eine Momentaufnahme aus einem Nur-anhängen-Repository zu löschen, aber ein Werkzeug, das sich nicht daran halten muss, könnte eine trotzdem direkt entfernen. Stellarshot bietet keine Möglichkeit, dies später wieder auszuschalten: „Platz freigeben“ und die „Behalten“-Einstellung oben funktionieren von da an nicht mehr. Wähle sorgfältig; von diesem Bildschirm aus gibt es kein Zurück.
wizard-remember-for-schedule = Geplante Sicherungen laufen nur mit gemerktem Passwort.
wizard-conditions = Bedingungen
wizard-require-ac = Nur am Netzstrom
wizard-require-ac-description = Eine geplante Sicherung überspringen, solange der Akku genutzt wird. Ein so ausgelassener Termin bleibt still; der nächste versucht es erneut.
wizard-min-battery = Mindest-Akkustand
wizard-battery-none = Kein Minimum
wizard-battery-percent = { $percent } %
wizard-block-metered = Nicht bei getakteter Verbindung
wizard-block-metered-description = Überspringen, solange das System die aktuelle Verbindung als getaktet markiert hat, etwa den mobilen Hotspot eines Telefons.
wizard-require-trusted-network = Nur in einem vertrauten Netzwerk
wizard-require-trusted-network-description = Überspringen, sofern keine Verbindung zu einem dieser WLANs besteht und kein VPN (Tailscale, WireGuard oder ein anderes) aktiv ist.
wizard-network-placeholder = Netzwerkname
wizard-hooks-title = Hooks für „{ $name }“
wizard-hooks-intro = Einen Befehl oder ein Programm vor und nach dieser Sicherung ausführen: zum Beispiel eine Datenbank vorher anhalten oder eine Netzwerkfreigabe danach aushängen. Wird wie ein Passwort-Befehl aufgeteilt, ohne eine echte Shell aufzurufen.
wizard-hooks-section = Hooks
wizard-hook-name-placeholder = Name
wizard-hook-command-placeholder = Befehl
hook-timing-before = Vor der Sicherung
hook-timing-after-success = Nach einer erfolgreichen Sicherung
hook-timing-after-failure = Nach einer fehlgeschlagenen Sicherung
hook-timing-after = Nach der Sicherung, so oder so
notify-backup-failed = Sicherung „{ $name }“ fehlgeschlagen
notify-cleanup-failed = Aufräumen von „{ $name }“ fehlgeschlagen
notify-check-failed = Prüfung von „{ $name }“ fehlgeschlagen
notify-open = Öffnen
error-timed-out = Innerhalb von { $seconds } Sekunden kam keine Antwort. Die Verbindung ist vielleicht langsam, oder der Speicherdienst begrenzt die Anfragen. Prüfe deine Verbindung und versuche es erneut.
error-conditions-not-met = Die Bedingungen wurden nicht erfüllt: { $reason }
error-hook-failed = Ein Hook ist fehlgeschlagen, daher wurde die Sicherung nicht ausgeführt: { $reason }
place-checking-for = Wird geprüft … { $time }
wizard-creating = Wird erstellt … { $time }
wizard-opening = Wird geöffnet … { $time }
wizard-saving = Wird gespeichert …
wizard-creating-note = Das Repository wird eingerichtet. Bei Cloud-Speicher kann das eine Minute dauern.
wizard-opening-note = Die Momentaufnahmen der Sicherung werden gelesen. Bei Cloud-Speicher kann das eine Minute dauern.
progress-uploaded = { $amount } · { $uploaded } gespeichert
progress-elapsed = Läuft seit { $time }
progress-waiting-preparing = Liest, was die Sicherung bereits enthält (bisher { $time }). Bei Cloud-Speicher kann das einige Minuten dauern.
progress-waiting = Seit { $time } hat sich nichts bewegt. Das passiert, wenn Cloud-Speicher Daten nur langsam annimmt oder Anfragen begrenzt; die Sicherung läuft von selbst weiter.
wizard-estimate-arithmetic = { $included } enthalten − { $excluded } ausgeschlossen = { $total }
wizard-estimate-nothing-excluded = Aus den Ordnern oben ist nichts ausgeschlossen.
wizard-estimate-adding-up = Wird gezählt … was die Ausschlüsse weglassen, wird zusammengerechnet.
wizard-patterns-remove = Diese Muster lassen { $size } weg, die die ausgeschlossenen Ordner nicht schon weglassen.
status-up-to-date = Aktuell
status-running = Läuft
status-overdue = Überfällig
status-failed = Fehlgeschlagen
status-damaged = Beschädigt
nav-running = { $name } — läuft …
nav-running-percent = { $name } — { $percent } %
menu-help = Hilfe
help = Hilfe
help-icons-title = Was die Symbole bedeuten
help-terms-title = Begriffe
term-repository = Repository
term-repository-description = Wo die verschlüsselten, deduplizierten Daten einer Sicherung liegen: ein Ordner, ein Laufwerk, ein Server oder ein Cloud-Speicher. Jedes Sicherungsprofil hat sein eigenes.
term-snapshot = Momentaufnahme
term-snapshot-description = Das Abbild deiner Dateien von einer Sicherung, zu einem bestimmten Zeitpunkt. Ein Repository enthält viele davon, und eine Wiederherstellung liest aus einer von ihnen.
term-rclone = rclone
term-rclone-description = Das eigenständige Programm, mit dem Stellarshot SSH-Server, Google Drive und andere Cloud-Speicher erreicht. Es ist nicht Teil von Stellarshot und hat seine eigene Konfiguration.
term-prune = Bereinigen
term-prune-description = Löscht Daten, die keine verbleibende Momentaufnahme mehr braucht, nachdem alte Momentaufnahmen vergessen wurden. „Automatisch Speicherplatz freigeben“ erledigt das für dich.
term-keep = Behalten
term-keep-description = Welche alten Momentaufnahmen beim Freigeben von Speicherplatz erhalten bleiben. „Intelligent“ behält einen schrumpfenden Verlauf; „Für immer“ behält alles, und die Sicherung wächst nur.
next-run = Nächste Sicherung: { $time }
summary-title = Ordner
summary-none = Keine
summary-included = Enthalten
summary-excluded = Ausgeschlossen
summary-freed = Durch Aufräumen freigegeben
statistics-title = Repository-Statistik
statistics-description = Die tatsächliche Größe im Speicher, das Kompressionsverhältnis und wie viel noch zurückgewonnen werden könnte. Liest jede Indexdatei und listet das Ziel auf.
statistics-calculate = Berechnen
statistics-calculating = Wird berechnet …
statistics-stored = Am Ziel gespeichert
statistics-ratio = Kompressionsverhältnis
statistics-no-ratio = Noch nicht bekannt
statistics-reclaimable = Könnte durch Aufräumen zurückgewonnen werden
history-title = Verlauf
event-backed-up = Gesichert
event-stage-backup = Sicherung
event-stage-check = Prüfung
event-stage-cleanup = Aufräumen
event-failed = { $stage } fehlgeschlagen: { $reason }
event-skipped = Übersprungen: { $reason }
event-checked-sound = Prüfung bestanden
event-checked-damaged = Prüfung hat Schäden gefunden
event-cleaned-up = { $count } Momentaufnahmen vergessen, { $size } freigegeben
event-restored = { $count ->
    [one] 1 Datei wiederhergestellt ({ $size })
   *[other] { $count } Dateien wiederhergestellt ({ $size })
}
event-snapshot-deleted = Momentaufnahme { $snapshot } gelöscht
event-pinned = Momentaufnahme { $snapshot } angeheftet
event-unpinned = Momentaufnahme { $snapshot } losgelöst
event-password-changed = Passwort geändert
event-mounted = Momentaufnahme { $snapshot } als Ordner eingebunden
event-unmounted = Momentaufnahme { $snapshot } ausgehängt
history-empty = Noch ist nichts passiert.
history-unknown-backup = (entfernte Sicherung)
history-truncated = Zeigt die neuesten { $shown } von { $total }
history-via-web = Web
notify-overdue = „{ $name }“ wurde lange nicht gesichert
notify-overdue-body = Das Ziel war zu den geplanten Zeiten nicht erreichbar. { $schedule } Prüfe, ob es verbunden ist, und öffne dann Stellarshot, um jetzt zu sichern.
settings-backup-title = Stellarshots eigene Einstellungen sichern und wiederherstellen
settings-export = Einstellungen exportieren
settings-export-description = Die Ordner, das Ziel und den Zeitplan jeder Sicherung, sowie ihr Verlauf. Nie ein Passwort.
settings-export-button = Exportieren …
settings-export-title = Stellarshots Einstellungen speichern
settings-export-done-title = Einstellungen exportiert
settings-export-done-body = Die Einstellungen und der Verlauf jeder Sicherung wurden in der gewählten Datei gespeichert.
settings-export-failed = Die Einstellungen konnten nicht gespeichert werden.
settings-import = Einstellungen importieren
settings-import-description = Sicherungen aus einem Einstellungsexport hinzufügen. Eine bereits vorhandene Sicherung bleibt genau so, wie sie ist; nur ihr Verlauf wird ergänzt.
settings-import-button = Importieren …
settings-import-title = Einen Einstellungsexport zum Importieren wählen
settings-import-done-title = Einstellungen importiert
settings-import-done-body = { $added ->
    [0] Es wurden keine neuen Sicherungen hinzugefügt.
    [1] Es wurde eine Sicherung hinzugefügt.
   *[other] Es wurden { $added } Sicherungen hinzugefügt.
} { $skipped ->
    [0] {""}
    [1] Eine war schon vorhanden und wurde unverändert gelassen.
   *[other] { $skipped } waren schon vorhanden und wurden unverändert gelassen.
}
settings-import-failed = Die Einstellungen konnten nicht importiert werden.
home = Übersicht
home-backups-title = Sicherungen
home-backup-detail = { $status } · { $last }
home-view = Anzeigen
home-folders-title = Auf diesem Computer gesicherte Ordner
home-locations-title = Speicherorte
home-location-detail = { $kind } · { $backups }
wizard-resume = Einrichtung fortsetzen
wizard-cancel-title = Die Einrichtung dieser Sicherung abbrechen?
wizard-cancel-body = Du kannst später in der Seitenleiste dazu zurückkehren, oder alles bisher Eingegebene verwerfen.
wizard-finish-later = Später fertigstellen
wizard-discard = Verwerfen
place-google-advanced = Meine eigenen Google-API-Zugangsdaten verwenden …
place-google-advanced-description = Melde dich mit einem eigenen Google-Cloud-Client an statt mit dem, den rclone mit allen teilt, die noch keinen eigenen eingerichtet haben. Braucht sowohl eine Client-ID als auch ein Client-Secret aus deinem eigenen Google-Cloud-Projekt; lass beides leer, um den geteilten Standard zu verwenden.
place-google-client-id = Client-ID
place-google-client-secret = Client-Secret
place-advanced = Erweitert
place-bandwidth-limit = Bandbreitenbegrenzung
place-bandwidth-limit-description = Begrenzt, wie schnell diese Sicherung hoch- und herunterlädt, in rclones eigener Syntax (1M, oder 8M:2M für Hochladen:Herunterladen). Leer für kein Limit.
place-bandwidth-limit-placeholder = z. B. 1M

# Panel applet
applet-tooltip = Stellarshot
applet-none = Es ist noch keine Sicherung eingerichtet.
applet-open = Stellarshot öffnen
wizard-exclude-caches = Cache-Ordner auslassen
wizard-exclude-caches-description = Ordner überspringen, die sich mit einer CACHEDIR.TAG-Datei als temporäre Cache-Daten kennzeichnen.
wizard-git-ignore = .gitignore beachten
wizard-git-ignore-description = Alles auslassen, was die eigene .gitignore-Datei jedes Projekts bereits ausschließt.
wizard-skip-if-unchanged = Leere Sicherungen überspringen
wizard-skip-if-unchanged-description = Keine neue Momentaufnahme aufzeichnen, wenn sich seit der letzten nichts geändert hat.
restore-advanced = Erweitert
restore-verify-existing = Vorhandene Dateien überprüfen
restore-verify-existing-description = Eine Datei, die bereits unverändert aussieht, lesen und prüfen, statt ihrer Größe und Änderungszeit zu vertrauen.
restore-ownership-preserve = Ursprünglichen Besitzer wiederherstellen
restore-ownership-numeric = Numerische Benutzer- und Gruppen-IDs wiederherstellen
restore-ownership-none = Besitzrechte nicht wiederherstellen
settings-cache-title = Lokaler Zwischenspeicher
settings-cache-dir = Speicherort des Zwischenspeichers
settings-cache-dir-default = Standard (~/.cache/rustic)
settings-cache-dir-choose = Auswählen …
settings-cache-dir-reset = Standard verwenden
settings-cache-dir-title = Ordner für den Zwischenspeicher wählen
settings-no-cache = Gar nicht zwischenspeichern
settings-no-cache-description = Langsamer, aber nichts, was es wert ist, auf einem Rechner mit wenig Speicherplatz behalten zu werden.
settings-global-excludes-title = Aus jeder Sicherung ausgelassen
settings-global-excludes-description = Glob-Muster wie node_modules oder target, die auf jede Sicherung angewendet werden, ohne sie jeder einzeln hinzuzufügen.
settings-web-title = Weboberfläche
web-scope-off = Aus
web-scope-off-description = Die Weboberfläche ist überhaupt nicht erreichbar.
web-scope-localhost = Nur dieser Computer
web-scope-localhost-description = Nur von diesem Computer selbst erreichbar, zum Beispiel über einen eigenen SSH-Tunnel.
web-scope-lan = Im Netzwerk erreichbar
web-scope-lan-description = Von jedem anderen Gerät im selben Netzwerk erreichbar.
web-auth-password = Gemeinsames Passwort
web-auth-password-description = Ein Passwort, getrennt vom Passwort jeder Sicherung, zum Anmelden erforderlich.
web-password-set = Passwort festlegen
web-password-placeholder = Neues Passwort
web-password-failed = Das Passwort der Weboberfläche konnte nicht gespeichert werden.
web-auth-token = API-Token
web-auth-token-description = Ein erzeugtes Token für die programmgesteuerte Nutzung der API der Weboberfläche.
web-token-generate = Token
web-token-generate-button = Neues Token erzeugen…
web-token-exists = Ein Token wurde erzeugt. Ein weiteres ersetzt es.
web-token-none = Es wurde noch kein Token erzeugt.
web-token-title = Neues API-Token
web-token-body = { $token }

Dies wird nur einmal angezeigt. Bewahre es sicher auf: ein weiteres erzeugtes Token ersetzt dieses.
web-auth-pam = Die eigene Anmeldung dieses Computers
web-auth-pam-description = Mit demselben Passwort anmelden, mit dem man sich an diesem Computer anmeldet.
web-allowed-title = Erlaubte Adressen
web-allowed-description = Nur diese Adressen oder Bereiche dürfen die Weboberfläche erreichen. Leer bedeutet jede Adresse, die die obige Einstellung bereits erlaubt.
web-allowed-placeholder = Adresse oder Bereich, zum Beispiel 192.168.1.0/24
pin-snapshot-failed = Die Pinnung der Momentaufnahme konnte nicht geändert werden.
pin-snapshot = Anheften, damit das Aufräumen diese Momentaufnahme nie entfernt
unpin-snapshot = Loslösen, damit das Aufräumen diese Momentaufnahme wieder entfernen kann
delete-snapshot-row = Diese Momentaufnahme löschen
