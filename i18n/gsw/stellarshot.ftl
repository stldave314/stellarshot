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

# Restore
restore-open = Widerherstelle …
restore-title = Widerherstelle us { $name }
restore-loading = D Sicherig wird göffnet …
tab-browse = Durchsueche
tab-deleted = Glöschti Dateie
tab-compare = Vergliiche
selected-count = { $count ->
    [one] 1 Element usgwählt
   *[other] { $count } Element usgwählt
}
restore-button = Widerherstelle …
search-placeholder = Die Momentuufnahm durchsueche
search-results = { $count ->
    [one] 1 Träffer
   *[other] { $count } Träffer
}
folder-empty = De Ordner isch leer.
versions-title = Versione
versions-same = { $count ->
    [one] ↳ i 1 älterer Momentuufnahm glich
   *[other] ↳ i { $count } ältere Momentuufnahme glich
}
open-copy = Kopie öffne
restore-this-version = Die Version widerherstelle …
deleted-scope = Dateie i { $folder }, wo i Sicherige vo de letschte { $days } Täg drin sind, aber nüme vorhande.
deleted-change-folder = Ordner ändere …
deleted-find = Glöschti Dateie sueche
restore-searching = Wird gsuecht …
deleted-intro = Suech nach glöschte Dateie, wo e Sicherig no hät.
deleted-none = Es fählt nüt: Jedi Datei us dene Sicherige isch no da.
deleted-last-seen = zletscht gsicheret { $when }
compare-button = Vergliiche
compare-intro = Wähl zwei Momentuufnahme, zum gseh, was sich gänderet hät.
compare-none = Zwüsche dene Momentuufnahme hät sich nüt gänderet.
compare-summary = { $added } dezue · { $removed } weg · { $changed } gänderet
restore-sheet-title = { $count ->
    [one] 1 Element widerherstelle
   *[other] { $count } Element widerherstelle
}
restore-to = Widerherstelle nach
restore-to-original = Wo si gsi sind
restore-to-folder = In en andere Ordner …
restore-to-folder-chosen = I { $folder }
restore-existing = Wänn e Datei scho existiert
policy-keep-both = Beidi bhalte
policy-keep-both-description = Di widerhergstellt Kopie überchunnt en neue Name; dini Datei blibt unberüert.
policy-overwrite = Überschriibe
policy-overwrite-description = Dur di gsicheret Kopie ersetze.
policy-skip = Überspringe
policy-skip-description = Bhalte und die Datei nöd widerherstelle.
restore-previewing = Es wird usegfunde, was passiert …
restore-choose-folder = Wähl de Ordner, wo widerhergstellt werde söll.
restore-preview-failed = Es hät nöd chöne usegfunde wärde, was d Widerherstellig mache würd.
preview-restore = { $count ->
    [one] 1 Datei wird widerhergstellt ({ $size })
   *[other] { $count } Dateie wärded widerhergstellt ({ $size })
}
preview-kept = { $count ->
    [one] 1 vorhandeni Datei isch anders und wird dernäbe bhalte
   *[other] { $count } vorhandeni Dateie sind anders und wärded dernäbe bhalte
}
preview-replaced = { $count ->
    [one] 1 vorhandeni Datei isch anders und wird ersetzt
   *[other] { $count } vorhandeni Dateie sind anders und wärded ersetzt
}
preview-skipped = { $count ->
    [one] 1 vorhandeni Datei isch anders und wird übersprunge
   *[other] { $count } vorhandeni Dateie sind anders und wärded übersprunge
}
preview-unchanged = { $count ->
    [one] 1 Datei isch scho glich und blibt unberüert
   *[other] { $count } Dateie sind scho glich und blibed unberüert
}
restore-done-title = Widerherstellig fertig
restore-done-body = { $count ->
    [one] 1 Datei widerhergstellt ({ $size }).
   *[other] { $count } Dateie widerhergstellt ({ $size }).
} { $conflicts ->
    [0] {""}
   *[other] Scho vorhandeni Dateie sind wie gwählt behandlet worde.
}
restore-failed = D Widerherstellig isch nöd fertig worde.
browse-failed = Die Sicherig hät nöd chöne gläse wärde.
open-copy-failed = Es hät kei Kopie vo dere Datei chöne göffnet wärde.
select-scope-folder = Wähl en Ordner zum Durchsueche
select-restore-folder = Wähl, wohi widerhergstellt werde söll

# Automation
error-password-not-remembered = Planti Sicherige bruuched s Passwort im Schlüsselbund. Öffne d Sicherig, gib ihres Passwort mit «Passwort merke» ii, denn lauft di nächst plant Sicherig.
change = Ändere …
schedule-row = Wänn si lauft
check-row = Uf Schäde prüefe
check-row-last = Zletscht prüeft { $when }
check-row-never = No nie prüeft
check-now = Jetzt prüefe
check-again = Nomal prüefe
check-failed = D Prüefig isch nöd fertig worde.
check-passed-title = Kei Schäde gfunde
check-passed-body = Jedi Momentuufnahm, jede Ordner und jede Indexiitrag vo dere Sicherig isch da und stimmt.
clean-up-row = Platz freigäh
clean-up-row-description = Vergisst Momentuufnahme, wo d Iistellig «Bhalte» nüme bruucht, und löscht Date, wo kei Momentuufnahm bruucht.
clean-up-now = Jetzt ufruume
clean-up-failed = S Ufruume isch nöd fertig worde.
clean-up-cannot-stop = S Freigäh vo Platz cha nach em Start nöd aghalte wärde.
clean-up-done-title = Ufruume fertig
clean-up-done-body = { $count ->
    [one] 1 Momentuufnahm vergässe.
   *[other] { $count } Momentuufnahme vergässe.
} { $size } wärded nüme bruucht.
progress-cleaning-up = Platz wird freigäh …
damaged-title = E Prüefig hät Schäde i dere Sicherig gfunde
damaged-body = S automatische Freigäh vo Platz isch pausiert, bis e Prüefig besteht. Momentuufnahme lönd sich vilicht no widerherstelle; sicherheitshalber fang e neui Sicherig amene andere Ort aa.
failed-just-now = grad eben
failed-minutes-ago = { $count ->
    [one] vor ere Minute
   *[other] vor { $count } Minute
}
failed-hours-ago = { $count ->
    [one] vor ere Stund
   *[other] vor { $count } Stunde
}
failed-days-ago = { $count ->
    [one] geschter
   *[other] vor { $count } Täg
}
scheduled-backup-failed = Di automatisch Sicherig isch fählgschlage ({ $when })
scheduled-cleanup-failed = S Ufruume nach de automatische Sicherig isch fählgschlage ({ $when })
scheduled-check-failed = Di automatisch Prüefig isch fählgschlage ({ $when })
schedule-failed = De Zitplan hät nöd chöne iigrichtet wärde.
schedule-manual = Sicheret nur, wänn du «Jetzt sichere» drucksch
schedule-hourly = Sicheret automatisch jedi Stund
schedule-daily = Sicheret automatisch jede Tag
schedule-weekly = Sicheret automatisch jedi Wuche
frequency-hourly = Jedi Stund
frequency-daily = Jede Tag
frequency-weekly = Jedi Wuche
keep-smart = Intelligent (empfohle)
keep-3-months = Mindestens 3 Mönet
keep-6-months = Mindestens 6 Mönet
keep-1-year = Mindestens es Jahr
keep-days = Mindestens { $days } Täg
keep-forever = Für immer
keep-smart-description = Ei Momentuufnahm pro Tag für e Wuche, eini pro Wuche für en Monet und eini pro Monet für es Jahr.
keep-forever-description = Jedi Momentuufnahm wird bhalte. D Sicherig wachst nur.
keep-for-description = Jedi Momentuufnahm us de { $days } Täg vor de neuschte, älteri wärded nach und nach entfernt.
wizard-schedule-title = Wänn «{ $name }» lauft
wizard-when-intro = Sicherige chönd vo sälber laufe. Isch de Computer denn us oder schlaft, lauft d Sicherig, sobald du zrugg bisch.
wizard-automatic = Automatisch sichere
wizard-automatic-description = Lauft im Hintergrund, au wänn Stellarshot zue isch.
wizard-frequency = Wie oft
wizard-keep = Alti Momentuufnahme
wizard-keep-label = Bhalte
wizard-prune = Platz automatisch freigäh
wizard-prune-description = Löscht Date, wo kei Momentuufnahm meh bruucht. Lahn das us, wänn en andere Computer an de gliich Ort sicheret.
wizard-remember-for-schedule = Planti Sicherige laufed nur mit gmerktem Passwort.
notify-backup-failed = Sicherig «{ $name }» fählgschlage
notify-cleanup-failed = Ufruume vo «{ $name }» fählgschlage
notify-check-failed = Prüefig vo «{ $name }» fählgschlage
notify-open = Öffne
