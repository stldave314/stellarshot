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
change-password-row = Passwort
change-password-row-description = Änderet s Passwort, womit d Sicherig entsperrt isch.
change-password-title = Passwort ändere
change-password-body = Gfüegt en Schlüssel für s neu Passwort dezue und entfernt de, womit du grad entsperrt bisch. De Schlüssel vo öpperem anders, oder vomene Wiederherstelligsblatt, blibt unberüert.
change-password-command-note = Du muesch's ou überall dert aktualisiere, wo s aktuell Passwort gspeicheret isch — das ändered nur d Sicherig sälber.
change-password-failed = S Passwort hät nöd chöne gänderet wärde.
password-source-row = Woher s Passwort chunnt
password-source-keyring = Im Schlüsselbund gspeicheret
password-source-command = Vo emne Befehl
password-source-title = Woher s Passwort chunnt
password-source-body = Es Befehl, wo s Passwort uf sinere Standardusgab uisgit, jedes Mal frisch usgführt, wenn eis bruucht wird, statt em Schlüsselbund — für en Passwortmanager mit emne Kommandozile-Client, zum Bispil d Bitwarden-CLI. Leer laa, zum de Schlüsselbund bruuche.
password-source-placeholder = z. B. bw get password stellarshot-home
password-command-failed = De Passwortbefähl hät nöd chöne usgfüehrt wärde.
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
browse-open = Duresueche …
browse-close = Schliesse
browse-scanning = Wird gscannt … bis jetzt { $count } gfunde
browse-mark-included = Iigschlosse
browse-mark-partial = Teilwiis iigschlosse
browse-mark-excluded = Usgschlosse
browse-include = Dää Ordner iischlüüsse
browse-exclude = Dää Ordner usschlüüsse
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
error-canceled = De Vorgang isch abbroche worde. Es isch nüt veränderet worde.
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
place-rest = REST-Server
place-rest-description = Es rest-server oder rustic-server, wo du selber betriibsch
place-rest-url = Server-URL
place-rest-url-placeholder = http://user:pass@host:8000/repo/
place-rclone-missing = Zum da sichere bruucht’s rclone. Installier’s (zum Bispiil mit sudo apt install rclone) und probier’s nomal.
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
folder-up = En Ordner uufe
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
download = Abelade …
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
download-title = Kopie us dere Sicherig spichere
download-failed = Es hät kei Kopie us dere Sicherig chöne gspeichert wärde.
select-scope-folder = Wähl en Ordner zum Durchsueche
select-restore-folder = Wähl, wohi widerhergstellt werde söll

# Automation
error-password-not-remembered = Planti Sicherige bruuched s Passwort im Schlüsselbund. Öffne d Sicherig, gib ihres Passwort mit «Passwort merke» ii, denn lauft di nächst plant Sicherig.
error-keyring-unavailable = S Passwort isch gänderet worde, hät aber nöd im Schlüsselbund gspeicheret chöne wärde: { $details }. Trag s dert sälber ii, süsch schlaht e planti Sicherig demit fähl.
error-delete-unsupported = Stellarshot cha d Date vo däm Ziel nöd sälber lösche. Entfern si dert sälber, oder bruuch «Entferne», zum si da z vergässe, ohni öppis z lösche.
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
keep-smart-description = Bhaltet di nöischti Momentuufnahm vo jedem vo de letschte 7 Täg, wo eini hät, vo jedere vo de letschte 4 Wuche (Mäntig bis Sunntig), wo eini hät, und vo jedem vo de letschte 12 Kalendermönet, wo eini hät. Täg, Wuche und Mönet ohni Sicherig wärded übersprunge, nöd zellt, und e Momentuufnahm cha grad di für ihre Tag, ihri Wuche und ihre Monet sii. Solang d Sicherige weniger als 12 Mönet umfassed, wird au di allererscht Momentuufnahm bhalte. Jedi anderi Momentuufnahm wird vergässe. Betroffe sind nur d Momentuufnahme vo dem Computer, und jedi gsichereti Ordnerwahl wird für sich zellt.
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
wizard-append-only = Nur aahänke
wizard-append-only-description = En Schutz gäge Fähler, nöd gäge Aagriff: rustic selber weigeret sich, e Momentuufnahm us emne Nur-aahänke-Repository z lösche, aber es Werkzüg, wo sich nöd draa halte muess, chönt eini troztdem direkt entferne. Stellarshot bütet kei Möglechkeit, das schpöter wieder abzschtelle: „Platz freimache“ und d „Bhalte“-Iistellig obe hörid vo däm Punkt a uf z funktioniere. Wähl sorgfältig; vo däm Bildschirm us gits kes Zrugg.
wizard-remember-for-schedule = Planti Sicherige laufed nur mit gmerktem Passwort.
notify-backup-failed = Sicherig «{ $name }» fählgschlage
notify-cleanup-failed = Ufruume vo «{ $name }» fählgschlage
notify-check-failed = Prüefig vo «{ $name }» fählgschlage
notify-open = Öffne
error-timed-out = Innert { $seconds } Sekunde isch kei Antwort cho. D Verbindig isch vilicht langsam, oder de Speicherdienscht begränzt d Aafrage. Lueg dini Verbindig aa und probier's nomal.
place-checking-for = Wird prüeft … { $time }
wizard-creating = Wird gmacht … { $time }
wizard-opening = Wird ufgmacht … { $time }
wizard-saving = Wird gspeicheret …
wizard-creating-note = S Repository wird iigrichtet. Bi Cloud-Speicher cha das e Minute gaa.
wizard-opening-note = D Momentuufnahme vo de Sicherig wärded gläse. Bi Cloud-Speicher cha das e Minute gaa.
progress-uploaded = { $amount } · { $uploaded } gspeicheret
progress-elapsed = Lauft sit { $time }
progress-waiting-preparing = Lieset, was d Sicherig scho enthaltet (bis jetzt { $time }). Bi Cloud-Speicher cha das es paar Minute gaa.
progress-waiting = Sit { $time } hät sich nüt bewegt. Das passiert, wänn de Cloud-Speicher Date nur langsam aanimmt oder d Aafrage begränzt; d Sicherig lauft vo sälber wiiter.
wizard-estimate-arithmetic = { $included } debii − { $excluded } usgschlosse = { $total }
wizard-estimate-nothing-excluded = Us de Ordner obe isch nüt usgschlosse.
wizard-estimate-adding-up = Wird zellt … was d Usschlüss weglönd, wird zämegrechnet.
wizard-patterns-remove = Die Muster lönd { $size } weg, wo d usgschlossene Ordner nöd scho weglönd.
status-up-to-date = Aktuell
status-running = Lauft
status-overdue = Überfällig
status-failed = Fehlgschlage
status-damaged = Beschädiget
nav-running = { $name } — lauft …
nav-running-percent = { $name } — { $percent } %
menu-help = Hilf
help = Hilf
help-icons-title = Was d Symbol bedüted
help-terms-title = Begriff
term-repository = Repository
term-repository-description = Wo d verschlüsselte, deduplizierte Date vo re Sicherig sind: en Ordner, e Platte, en Server oder Cloud-Speicher. Jedes Sicherigsprofil hät sis eiget.
term-snapshot = Momentuufnahm
term-snapshot-description = S Abbild vo dine Datene vo re Sicherig, zu emne bstimmte Zitpunkt. Es Repository hät vieli dervo, und e Widerherstellig lieset us eire vo ene.
term-rclone = rclone
term-rclone-description = S eigeständige Programm, wo Sterneschuss nutzt für SSH-Server, Google Drive und anderi Cloud-Speicher z erreiche. Es isch nöd Teil vo Sterneschuss und hät si eigeti Konfiguration.
term-prune = Bereinige
term-prune-description = Lösched Date, wo kei verbliebeni Momentuufnahm me bruucht, nachdem alti Momentuufnahme vergässe worde sind. „Automatisch Platz freigeh“ macht das für dich.
term-keep = Bhalte
term-keep-description = Weli alti Momentuufnahme bliebe, wänn Platz freigeh wird. „Intelligent“ bhaltet e chliner werdendi Gschicht; „Für immer“ bhaltet alles, und d Sicherig wachst nur.
next-run = Nächsti Sicherig: { $time }
summary-title = Ordner
summary-none = Kei
summary-included = Debii
summary-excluded = Usgschlosse
summary-freed = Freigeh dur Ufruume
statistics-title = Repository-Statistik
statistics-description = Di tatsächlichi Grössi im Speicher, s Kompressionsverhältnis, und wie viel no chönt zrückgwunne wärde. Lieset jedi Indexdatei und lischtet s Ziel uf.
statistics-calculate = Bereschne
statistics-calculating = Wird bereschnet …
statistics-stored = Am Ziel gspeicheret
statistics-ratio = Kompressionsverhältnis
statistics-no-ratio = No nöd bekannt
statistics-reclaimable = Chönt dur Ufruume zrückgwunne wärde
history-title = Verlauf
event-backed-up = Gsicheret
event-stage-backup = Sicherig
event-stage-check = Prüefig
event-stage-cleanup = Ufruume
event-failed = { $stage } fehlgschlage: { $reason }
event-skipped = Übersprunge: { $reason }
event-checked-sound = Prüefig bestande
event-checked-damaged = Prüefig hät Schäde gfunde
event-cleaned-up = { $count } Momentuufnahme vergässe, { $size } freigeh
notify-overdue = „{ $name }“ isch scho lang nüm gsicheret worde
notify-overdue-body = Ziel isch zu de vorgsehene Zite nöd erreichbar gsi. { $schedule } Lueg, öb's verbunde isch, und mach de Sterneschuss uf, zum jetzt z sichere.
settings-backup-title = Sterneschuss sini eigete Iistellige sichere und widerherstelle
settings-export = Iistellige exportiere
settings-export-description = D Ordner, s Ziel und de Zitplan vo jeder Sicherig, sowie ihre Verlauf. Nie es Passwort.
settings-export-button = Exportiere …
settings-export-title = Sterneschuss sini Iistellige spichere
settings-export-done-title = Iistellige exportiert
settings-export-done-body = D Iistellige und de Verlauf vo jeder Sicherig sind i de gwählte Datei gspeicheret worde.
settings-export-failed = D Iistellige händ nöd chöne gspeicheret wärde.
settings-import = Iistellige importiere
settings-import-description = Sicherige us emne Iistellige-Export dezuefüege. Ei, wo scho da isch, blibt gnau so wie si isch; nur ihre Verlauf wird ergänzt.
settings-import-button = Importiere …
settings-import-title = En Iistellige-Export zum Importiere wähle
settings-import-done-title = Iistellige importiert
settings-import-done-body = { $added ->
    [0] Es sind kei neui Sicherige dezuecho.
    [1] Es isch eini Sicherig dezuecho.
   *[other] Es sind { $added } Sicherige dezuecho.
} { $skipped ->
    [0] {""}
    [1] Eini isch scho da gsi und isch unverändert bliebe.
   *[other] { $skipped } sind scho da gsi und sind unverändert bliebe.
}
settings-import-failed = D Iistellige händ nöd chöne importiert wärde.
home = Übersicht
home-backups-title = Sicherige
home-backup-detail = { $status } · { $last }
home-view = Azeige
home-folders-title = Ordner, wo uf däm Computer gsicheret wärde
home-locations-title = Speicherört
home-location-detail = { $kind } · { $backups }
wizard-resume = Yrichtig widerufnäh
wizard-cancel-title = D Yrichtig vo däre Sicherig abbräche?
wizard-cancel-body = Du chasch später i de Sytiliste druf zrugg cho, oder alles bisher Yigäh verwerfe.
wizard-finish-later = Später fertig mache
wizard-discard = Verwerfe
place-google-advanced = Mini eigete Google-API-Zuedaate bruuche …
place-google-advanced-description = Mäld dich a mit emne eigete Google-Cloud-Client, statt mit däm, wo rclone mit alle teilt, wo no kei eigete yrichtet händ. Bruucht sowohl e Client-ID als au es Client-Secret vo dim eigete Google-Cloud-Projekt; lah beides leer, zum de gteilte Standard bruuche.
place-google-client-id = Client-ID
place-google-client-secret = Client-Secret
place-advanced = Erwiteret
place-bandwidth-limit = Bandbreiti-Limit
place-bandwidth-limit-description = Begrenzt, wie schnell die Sicherig uf- und abeladed, i rclone sinere eigete Syntax (1M, oder 8M:2M für Uflade:Abelade). Leer für kes Limit.
place-bandwidth-limit-placeholder = z. B. 1M
wizard-exclude-caches = Cache-Ordner uslah
wizard-exclude-caches-description = Ordner überspringe, wo sich mit re CACHEDIR.TAG-Datei als temporäri Cache-Date kennzeichnet.
wizard-git-ignore = .gitignore beachte
wizard-git-ignore-description = Alles uslah, wo die eigeti .gitignore-Datei vo jedem Projekt scho usschliesst.
wizard-skip-if-unchanged = Leeri Sicherige überspringe
wizard-skip-if-unchanged-description = Kei neui Momentuufnahm ufzeichne, wenn sich sit de letschte nüt gänderet hät.
restore-advanced = Erwiteret
restore-verify-existing = Vorhandeni Dateie überprüefe
restore-verify-existing-description = E Datei, wo scho unverändert usgseht, lise und prüefe, statt ihrer Grössi und Änderigszit z vertroue.
restore-ownership-preserve = Ursprünglige Bsitzer widerherstelle
restore-ownership-numeric = Numerischi Benutzer- und Gruppe-IDs widerherstelle
restore-ownership-none = Bsitzrächt nöd widerherstelle
settings-cache-title = Lokale Zwüschespeicher
settings-cache-dir = Ort vom Zwüschespeicher
settings-cache-dir-default = Standard (~/.cache/rustic)
settings-cache-dir-choose = Uswähle …
settings-cache-dir-reset = Standard bruuche
settings-cache-dir-title = Ordner für de Zwüschespeicher wähle
settings-no-cache = Gar nüt zwüscheschpeichere
settings-no-cache-description = Langsamer, aber nüt, wo sich uf emne Computer mit wenig Speicherplatz lohnt z bhalte.
settings-global-excludes-title = Us jeder Sicherig uslah
settings-global-excludes-description = Glob-Muster wie node_modules oder target, wo uf jedi Sicherig aagwendet wärde, ohni si jedere einzelne dezuezfüege.
pin-snapshot-failed = D Aaghänkig vo de Momentuufnahm hät nöd chöne gänderet wärde.
pin-snapshot = Aahefte, dass s Ufruume die Momentuufnahm nie entfernt
unpin-snapshot = Lösmache, dass s Ufruume die Momentuufnahm wieder chan entferne
delete-snapshot-row = Die Momentuufnahm lösche
