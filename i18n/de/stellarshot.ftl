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
error-cancelled = Der Vorgang wurde abgebrochen. Es wurde nichts verändert.
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
