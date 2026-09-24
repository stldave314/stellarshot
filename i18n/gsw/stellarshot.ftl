# Application
stellarshot = Sterneschuss

# Empty state
empty-title = Heb dini Dateie sicher uf
empty-body = Sicher dini Ordner uf es anders Laufwerk oder in en andere Ordner. D Sicherige sind verschlüsslet, und nach de erschte wärded nur no Änderige gspeicheret.
create-backup = Sicherig mache …
open-existing = E vorhandeni Sicherig öffne

# Profile page
never-backed-up = No nie gsicheret
backed-up-just-now = Grad vorhin gsicheret
backed-up-minutes-ago = Letschti Sicherig vor { $count ->
    [one] ere Minute
   *[other] { $count } Minute
}
backed-up-hours-ago = Letschti Sicherig vor { $count ->
    [one] ere Stund
   *[other] { $count } Stunde
}
backed-up-days-ago = Letschti Sicherig vor { $count ->
    [one] eme Tag
   *[other] { $count } Täg
}
status-detail = { $destination } · { $count ->
    [one] ei Momentuufnahm
   *[other] { $count } Momentuufnahme
}
back-up-now = Jetzt sichere
choose-what-title = Wähl, was gsicheret wird
choose-what-body = Für die Sicherig sind no kei Ordner usgwählt.
choose-what-button = Ordner wähle …
unlock-title = Gib s Passwort vo dere Sicherig ii
unlock = Entsperre
remember-password = Passwort merke
remember-password-description = Wird i dim Schlüsselbund gspeicheret. Planti Sicherige bruuched s.
recent-snapshots = Letschti Momentuufnahme
no-snapshots-yet = No kei Momentuufnahme. Druck „Jetzt sichere“ für di erscht.
snapshot-row = { $id } · { $size } · { $added } neu
show-all-snapshots = All { $count } Momentuufnahme zeige
manage = Verwalte
edit-backup = Was gsicheret wird
edit-backup-description = Ii- und usgschlosseni Ordner ändere.
remove-backup = Us Sterneschuss entferne
remove-backup-description = Sterneschuss vergisst die Sicherig. D Sicherig sälber blibt, wo sie isch.
delete-backup = Sicherig und alli Date lösche
delete-backup-description = Löscht jedi Momentuufnahm vo dere Sicherig für immer.
progress-starting = Fangt a …
progress-preparing = Wird vorbereitet …
progress-backing-up = Wird gsicheret …
progress-restoring = Wird widerhergstellt …
progress-checking = Wird prüeft …
progress-amount = { $done } vo { $total }

# Wizard
wizard-create-title = Neui Sicherig
wizard-open-title = E vorhandeni Sicherig öffne
wizard-edit-title = Was „{ $name }“ sicheret
wizard-step = Schritt { $current } vo { $total }
wizard-what-intro = Wähl d Ordner zum Sichere und was drin uusglah wärde söll.
wizard-include = Iischlüüsse
wizard-exclude = Usschlüüsse
wizard-exclude-outside = Isch i keim iigschlossene Ordner und ändert drum nüt
wizard-add-folders = Ordner hinzuefüege …
wizard-pick-sources = Ordner zum Sichere wähle
wizard-pick-excludes = Ordner zum Uuslah wähle
wizard-advanced = Erwiiteret
wizard-pattern-placeholder = Passendi Näme uuslah, z. B. *.tmp oder node_modules
wizard-one-file-system = Uf em gliiche Laufwerk bliibe
wizard-one-file-system-description = Keine andere Laufwerk oder Netzwerk-Ordner folge, wo i dene Ordner iighänkt sind.
wizard-estimate-label = Gschätzti Grössi vo de Sicherig
wizard-estimate = { $size } · { $files } Dateie
wizard-estimate-counting = Wird zellt …
wizard-estimate-note = Di erscht Sicherig isch nach Kompression und Deduplizierig meischtens chliiner. Spöteri Sicherige speichered nur Änderige.
wizard-where-intro = Wähl, wo d Sicherig uufbewahrt wird: en läre Ordner, am beschte uf emne andere Laufwerk.
wizard-where-title = Speicherort
wizard-no-folder = Kei Ordner gwählt
wizard-choose-folder = Ordner wähle …
select-repo-folder = Wähl en Ordner für s Archiv
wizard-where-new = Da wird e neui Sicherig gmacht.
wizard-where-existing = De Ordner hät scho e Sicherig drin. Füeg sie statt dem über „E vorhandeni Sicherig öffne“ hinzue.
wizard-where-found = Da isch e Sicherig gfunde worde.
wizard-where-no-repository = I dem Ordner git’s kei Sicherig.
wizard-where-not-empty = De Ordner hät scho anderi Dateie drin. Wähl en läre Ordner.
wizard-name = Name
wizard-name-placeholder = Zum Bispiil: Persönliche Ordner uf USB-Laufwerk
wizard-secure-intro = Wähl es Passwort. Dini Sicherig wird demit verschlüsslet.
wizard-open-intro = Gib s Passwort ii, mit dem die Sicherig gmacht worde isch.
wizard-confirm = Passwort bstätige
wizard-mismatch = D Passwörter stimmed nöd überii.
wizard-password-warning = Wänn du das Passwort verliersch, chönd dini Sicherige nöd widerhergstellt wärde. Niemert cha’s für dich widerhole.
wizard-finish-create = Mache und jetzt sichere
wizard-finish-open = Öffne

# Dialogs and buttons
ok = OK
save = Speichere
add = Hinzuefüege
back = Zrugg
next = Wiiter
edit = Bearbeite
remove = Entferne
delete = Lösche
cancel = Abbreche
password = Passwort
remove-title = „{ $name }“ entferne?
remove-body = Sterneschuss vergisst die Sicherig und s gspeicherete Passwort. D Sicherig und iri Momentuufnahme wärded nöd glöscht und chönd spöter wider göffnet wärde.
delete-title = „{ $name }“ und alli Date lösche?
delete-body = Das löscht d Sicherig und jedi Momentuufnahm drin für immer. Anderi Dateie im gliiche Ordner blibed unberüert. Gib { $name } zum Bstätige ii.

# Errors
error-title = Öppis isch schiefgange
error-details = Details: { $details }
location-not-empty = { $path } hät scho anderi Dateie drin. Wähl en läre Ordner oder en Ordner, wo scho es Archiv drin hät.
create-repo-failed = S Archiv hät nöd chöne gmacht wärde.
delete-repo-failed = S Archiv hät nöd chöne glöscht wärde.
delete-snapshot-failed = D Momentuufnahm hät nöd chöne glöscht wärde.
snapshot-failed = D Momentuufnahm hät nöd chöne gmacht wärde.
open-repo-failed = S Archiv hät nöd chöne göffnet wärde.
error-wrong-password = S Passwort isch falsch.
error-not-a-repository = Under { $path } git's kei Archiv.
error-already-exists = { $path } hät scho es Archiv drin.
error-destination-unavailable = { $path } isch nöd erreichbar. Wänn's uf emne Wächseldatenträger oder emne Netzwerk-Ordner isch, lueg, öb's verbunde isch.
error-locked = Es anders Sichere bruucht das Archiv grad. Probier's nomal, wänn's fertig isch.
error-cancelled = De Vorgang isch abbroche worde. Es isch nüt veränderet worde.
error-repository-damaged = D Prüefig vom Archiv hät Problem gfunde. Lösch kei anderi Kopie vo dine Date, bis das glöst isch.

# About
about = Über
about-author = D Stellarshot-Mitwirkende
about-credits = Basiert uf Stellarshot vom cosmic-utils-Projekt.
repository = Quellcode
support = Hilf

# Settings
settings = Iistellige
appearance = Ussehe
theme = Färbig
match-desktop = A de Desktop agliche
dark = Dunkel
light = Hell

# Menu
file = Datei
menu-new-backup = Neui Sicherig …
new-backup = Neui Sicherig
new-window = Neus Fenster
quit = Verlah
view = Asicht
menu-settings = Iinstellige
menu-about = Informatione über Sterneschuss

# Storage locations
google-drive = Google Drive
place-folder = Ordner
place-folder-description = En Ordner uf dem Computer oder emne iighänkte Laufwerk
place-drive = Wächseldatenträger
place-drive-description = Es USB-Laufwerk, egal wo’s iighänkt isch
place-server = Netzwerkserver (SFTP)
place-server-description = En Ordner uf emne Computer, wo du per SSH erreichsch
place-google-description = Dis Google-Konto, aagmäldet i Sterneschuss
place-remote = Eis vo dine rclone-Remotes
place-remote-description = OneDrive, Dropbox, S3 und alles anderi, wo rclone erreicht
place-rclone-missing = Zum da sichere bruucht’s rclone. Installier’s (zum Bispiil mit sudo apt install rclone) und probier’s nomal.
place-checking = Wird prüeft …
place-check-failed = De Speicherort hät nöd chöne prüeft wärde.
place-check = Prüefe
place-no-drives = Es sind kei Wächseldatenträger aagschlosse. Schlüss eine aa und gang dänn zrugg und wider wiiter.
place-folder-on-drive = Ordner uf em Laufwerk
place-host = Server
place-user = Benutzername
place-user-placeholder = Din Benutzername uf dem Computer
place-port = Port
place-server-path = Ordner uf em Server
place-server-note = D Aamäldig lauft über din SSH-Agent oder dini Schlüssel, und de Server muess scho i ~/.ssh/known_hosts staa: Verbind dich zerscht eimal per ssh.
place-signing-in = Schlüss d Aamäldig im Browser ab. Sterneschuss wartet …
place-google-intro = Sterneschuss macht din Browser uf, damit du dich bi Google aamälde chasch. D Aamäldig wird nur i de eigete Iistellige vo Sterneschuss gspeicheret.
place-sign-in = Mit Google aamälde …
place-signed-in = Aagmäldet.
place-cloud-folder = Ordner
place-no-remotes = Du häsch kei rclone-Remotes. Richt eis mit rclone config ii und chumm dänn zrugg.
error-rclone-missing = rclone isch nöd installiert. Installier’s (zum Bispiil mit sudo apt install rclone), zum de Speicherort bruuche.
error-auth-failed = D Aamäldig isch nöd abgschlosse worde: { $details }

# Déjà Dup import
dejadup-import = Us Déjà Dup importiere
dejadup-title = E Déjà-Dup-Sicherig importiere
dejadup-name = Déjà-Dup-Sicherig
dejadup-none = Es sind kei Déjà-Dup-Iistellige gfunde worde.
dejadup-other-format = Die Déjà-Dup-Sicherig bruucht s älteri duplicity-Format, wo Sterneschuss nöd läse cha. Mach statt dem e neui Sicherig; di alt blibt i Déjà Dup lesbar.
dejadup-unsupported = Déjà Dup bhaltet die Sicherig amene Ort, wo Sterneschuss nöd bruuche cha ({ $backend }). Mach statt dem e neui Sicherig.
menu-import-dejadup = Us Déjà Dup importiere …
