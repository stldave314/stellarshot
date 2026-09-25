# Application
stellarshot = Stellarshot

# Empty state
empty-title = Håll dina filer säkra
empty-body = Säkerhetskopiera dina mappar till en annan enhet eller mapp. Säkerhetskopiorna är krypterade, och efter den första sparas bara ändringar.
create-backup = Skapa en säkerhetskopia…
open-existing = Öppna en befintlig säkerhetskopia

# Profile page
never-backed-up = Inte säkerhetskopierad än
backed-up-just-now = Senaste säkerhetskopian nyss
backed-up-minutes-ago = Senaste säkerhetskopian för { $count ->
    [one] en minut
   *[other] { $count } minuter
} sedan
backed-up-hours-ago = Senaste säkerhetskopian för { $count ->
    [one] en timme
   *[other] { $count } timmar
} sedan
backed-up-days-ago = Senaste säkerhetskopian för { $count ->
    [one] en dag
   *[other] { $count } dagar
} sedan
status-detail = { $destination } · { $count ->
    [one] en ögonblicksbild
   *[other] { $count } ögonblicksbilder
}
back-up-now = Säkerhetskopiera nu
choose-what-title = Välj vad som ska säkerhetskopieras
choose-what-body = Den här säkerhetskopian har inga mappar att säkerhetskopiera än.
choose-what-button = Välj mappar…
unlock-title = Ange lösenordet för den här säkerhetskopian
unlock = Lås upp
remember-password = Kom ihåg lösenordet
remember-password-description = Sparas i din nyckelring. Schemalagda säkerhetskopieringar behöver det.
recent-snapshots = Senaste ögonblicksbilderna
no-snapshots-yet = Inga ögonblicksbilder än. Tryck på Säkerhetskopiera nu för att ta den första.
snapshot-row = { $id } · { $size } · { $added } nytt
show-all-snapshots = Visa alla { $count } ögonblicksbilder
manage = Hantera
edit-backup = Vad som säkerhetskopieras
edit-backup-description = Ändra de inkluderade och exkluderade mapparna.
change-password-row = Lösenord
change-password-row-description = Ändra lösenordet den här säkerhetskopian är upplåst med.
change-password-title = Ändra lösenord
change-password-body = Lägger till en nyckel för det nya lösenordet och tar sedan bort den du är upplåst med nu. Någon annans nyckel, eller ett återställningsblads, lämnas orörd.
change-password-command-note = Du måste också uppdatera det överallt där det nuvarande lösenordet är sparat — detta ändrar bara själva säkerhetskopian.
change-password-failed = Lösenordet kunde inte ändras.
password-source-row = Hur lösenordet anges
password-source-keyring = Sparat i nyckelringen
password-source-command = Från ett kommando
password-source-title = Hur lösenordet anges
password-source-body = Ett kommando som skriver ut lösenordet på sin standardutdata, kört på nytt varje gång ett behövs, i stället för nyckelringen — för en lösenordshanterare med ett kommandoradsverktyg, till exempel Bitwarden CLI. Lämna tomt för att använda nyckelringen istället.
password-source-placeholder = t.ex. bw get password stellarshot-home
password-command-failed = Lösenordskommandot kunde inte köras.
remove-backup = Ta bort från Stellarshot
remove-backup-description = Stellarshot glömmer den här säkerhetskopian. Själva säkerhetskopian ligger kvar.
delete-backup = Radera säkerhetskopian och all data
delete-backup-description = Raderar permanent alla ögonblicksbilder i den här säkerhetskopian.
progress-starting = Startar…
progress-preparing = Förbereder…
progress-backing-up = Säkerhetskopierar…
progress-restoring = Återställer…
progress-checking = Kontrollerar…
progress-amount = { $done } av { $total }

# Wizard
wizard-create-title = Ny säkerhetskopia
wizard-open-title = Öppna en befintlig säkerhetskopia
wizard-edit-title = Vad ”{ $name }” säkerhetskopierar
wizard-step = Steg { $current } av { $total }
wizard-what-intro = Välj mapparna som ska säkerhetskopieras, och vad i dem som ska utelämnas.
wizard-include = Inkludera
wizard-exclude = Exkludera
wizard-exclude-outside = Ligger inte i en inkluderad mapp, så den ändrar ingenting
wizard-add-folders = Lägg till mappar…
wizard-pick-sources = Välj mappar att säkerhetskopiera
wizard-pick-excludes = Välj mappar att utelämna
wizard-advanced = Avancerat
wizard-pattern-placeholder = Utelämna namn som matchar, t.ex. *.tmp eller node_modules
wizard-one-file-system = Stanna på samma enhet
wizard-one-file-system-description = Följ inte in i andra enheter eller nätverksresurser som är monterade i de här mapparna.
wizard-estimate-label = Uppskattad storlek
wizard-estimate = { $size } · { $files } filer
wizard-estimate-counting = Räknar…
wizard-estimate-note = Den första säkerhetskopian blir oftast mindre efter komprimering och deduplicering. Senare säkerhetskopior sparar bara det som ändrats.
wizard-where-intro = Välj var säkerhetskopian ska förvaras: en tom mapp, helst på en annan enhet.
wizard-where-title = Plats
wizard-no-folder = Ingen mapp vald
wizard-choose-folder = Välj mapp…
select-repo-folder = Välj en mapp för lagringsplatsen
wizard-where-new = En ny säkerhetskopia skapas här.
wizard-where-existing = Den här mappen innehåller redan en säkerhetskopia. Använd ”Öppna en befintlig säkerhetskopia” för att lägga till den i stället.
wizard-where-found = En säkerhetskopia hittades här.
wizard-where-no-repository = Det finns ingen säkerhetskopia i den här mappen.
wizard-where-not-empty = Den här mappen innehåller redan andra filer. Välj en tom mapp.
wizard-name = Namn
wizard-name-placeholder = Till exempel: Hemmapp till USB-enhet
wizard-secure-intro = Välj ett lösenord. Din säkerhetskopia krypteras med det.
wizard-open-intro = Ange lösenordet som säkerhetskopian skapades med.
wizard-confirm = Bekräfta lösenordet
wizard-mismatch = Lösenorden stämmer inte överens.
wizard-password-warning = Om du tappar bort lösenordet kan dina säkerhetskopior inte återställas. Ingen kan återskapa det åt dig.
wizard-finish-create = Skapa och säkerhetskopiera nu
wizard-finish-open = Öppna

# Dialogs and buttons
ok = Ok
save = Spara
add = Lägg till
back = Tillbaka
next = Nästa
edit = Editera
remove = Ta bort
delete = Ta bort
cancel = Avbryt
password = Lösenord
remove-title = Ta bort ”{ $name }”?
remove-body = Stellarshot glömmer den här säkerhetskopian och dess sparade lösenord. Säkerhetskopian och dess ögonblicksbilder raderas inte och kan öppnas igen senare.
delete-title = Radera ”{ $name }” och all data?
delete-body = Detta raderar säkerhetskopian och alla ögonblicksbilder i den permanent. Andra filer i samma mapp påverkas inte. Skriv { $name } för att bekräfta.

# Errors
error-title = Något gick fel
error-details = Detaljer: { $details }
location-not-empty = { $path } innehåller redan andra filer. Välj en tom mapp, eller en mapp som redan innehåller en lagringsplats.
create-repo-failed = Lagringsplatsen kunde inte skapas.
delete-repo-failed = Lagringsplatsen kunde inte tas bort.
delete-snapshot-failed = Ögonblicksbilden kunde inte tas bort.
snapshot-failed = Ögonblicksbilden kunde inte skapas.
open-repo-failed = Lagringsplatsen kunde inte öppnas.
error-wrong-password = Lösenordet är fel.
error-not-a-repository = Det finns ingen lagringsplats i { $path }.
error-already-exists = { $path } innehåller redan en lagringsplats.
error-destination-unavailable = { $path } kan inte nås. Om den finns på en flyttbar enhet eller en nätverksresurs, kontrollera att den är ansluten.
error-locked = En annan säkerhetskopiering använder redan den här lagringsplatsen. Försök igen när den är klar.
error-cancelled = Åtgärden avbröts. Ingenting ändrades.
error-repository-damaged = Kontrollen av lagringsplatsen hittade problem. Ta inte bort andra kopior av dina data förrän detta är löst.

# About
about = Om
about-author = Stellarshots bidragsgivare
about-credits = Baserat på Stellarshot från projektet cosmic-utils.
repository = Källkod
support = Support

# Settings
settings = Inställningar
appearance = Utseende
theme = Tema
match-desktop = Matcha skrivbordet
dark = Mörkt
light = Ljust

# Menu
file = Fil
menu-new-backup = Ny säkerhetskopia…
new-backup = Ny säkerhetskopia
new-window = Nytt fönster
quit = Avsluta
view = Visa
menu-settings = Inställningar...
menu-about = Om Stellarshot...

# Storage locations
google-drive = Google Drive
place-folder = Mapp
place-folder-description = En mapp på den här datorn eller en monterad enhet
place-drive = Flyttbar enhet
place-drive-description = En USB-enhet, var den än monteras
place-server = Nätverksserver (SFTP)
place-server-description = En mapp på en dator du når med SSH
place-google-description = Ditt Google-konto, inloggat från Stellarshot
place-remote = En av dina rclone-fjärrplatser
place-remote-description = OneDrive, Dropbox, S3 och allt annat rclone kan nå
place-rest = REST-server
place-rest-description = En rest-server eller rustic-server som du kör själv
place-rest-url = Server-URL
place-rest-url-placeholder = http://user:pass@host:8000/repo/
place-rclone-missing = För att säkerhetskopiera hit behövs rclone. Installera det (till exempel med sudo apt install rclone) och försök igen.
place-check-failed = Platsen kunde inte kontrolleras.
place-check = Kontrollera
place-no-drives = Inga flyttbara enheter är anslutna. Anslut en och gå sedan tillbaka och framåt igen.
place-folder-on-drive = Mapp på enheten
place-host = Server
place-user = Användarnamn
place-user-placeholder = Ditt användarnamn på den datorn
place-port = Port
place-server-path = Mapp på servern
place-server-note = Inloggningen använder din SSH-agent eller dina nycklar, och servern måste redan finnas i ~/.ssh/known_hosts: anslut till den med ssh en gång först.
place-signing-in = Slutför inloggningen i webbläsaren. Stellarshot väntar…
place-google-intro = Stellarshot öppnar din webbläsare så att du kan logga in hos Google. Bara Stellarshots egna inställningar sparar inloggningen.
place-sign-in = Logga in med Google…
place-signed-in = Inloggad.
place-cloud-folder = Mapp
place-no-remotes = Du har inga rclone-fjärrplatser. Skapa en med rclone config och kom sedan tillbaka.
error-rclone-missing = rclone är inte installerat. Installera det (till exempel med sudo apt install rclone) för att använda den här platsen.
error-auth-failed = Inloggningen slutfördes inte: { $details }

# Déjà Dup import
dejadup-import = Importera från Déjà Dup
dejadup-title = Importera en Déjà Dup-säkerhetskopia
dejadup-name = Déjà Dup-säkerhetskopia
dejadup-none = Inga inställningar för Déjà Dup hittades.
dejadup-other-format = Den här Déjà Dup-säkerhetskopian använder det äldre duplicity-formatet, som Stellarshot inte kan läsa. Skapa en ny säkerhetskopia i stället; den gamla går fortfarande att läsa i Déjà Dup.
dejadup-unsupported = Déjà Dup förvarar den här säkerhetskopian på en plats som Stellarshot inte kan använda ({ $backend }). Skapa en ny säkerhetskopia i stället.
menu-import-dejadup = Importera från Déjà Dup…

# Restore
restore-open = Återställ…
restore-title = Återställ från { $name }
restore-loading = Öppnar säkerhetskopian…
folder-up = En mapp upp
tab-browse = Bläddra
tab-deleted = Borttagna filer
tab-compare = Jämför
selected-count = { $count ->
    [one] 1 objekt markerat
   *[other] { $count } objekt markerade
}
restore-button = Återställ…
search-placeholder = Sök i den här ögonblicksbilden
search-results = { $count ->
    [one] 1 träff
   *[other] { $count } träffar
}
folder-empty = Den här mappen är tom.
versions-title = Versioner
versions-same = { $count ->
    [one] ↳ likadan i 1 äldre ögonblicksbild
   *[other] ↳ likadan i { $count } äldre ögonblicksbilder
}
open-copy = Öppna kopia
restore-this-version = Återställ den här versionen…
deleted-scope = Filer i { $folder } som finns i säkerhetskopior från de senaste { $days } dagarna men inte längre finns på disken.
deleted-change-folder = Byt mapp…
deleted-find = Hitta borttagna filer
restore-searching = Letar…
deleted-intro = Leta efter filer du tagit bort som en säkerhetskopia fortfarande har.
deleted-none = Inget saknas: varje fil i de här säkerhetskopiorna finns kvar.
deleted-last-seen = senast säkerhetskopierad { $when }
compare-button = Jämför
compare-intro = Välj två ögonblicksbilder för att se vad som ändrats.
compare-none = Inget ändrades mellan de här ögonblicksbilderna.
compare-summary = { $added } tillagda · { $removed } borttagna · { $changed } ändrade
restore-sheet-title = { $count ->
    [one] Återställ 1 objekt
   *[other] Återställ { $count } objekt
}
restore-to = Återställ till
restore-to-original = Där de var
restore-to-folder = En annan mapp…
restore-to-folder-chosen = Till { $folder }
restore-existing = Om en fil redan finns
policy-keep-both = Behåll båda
policy-keep-both-description = Den återställda kopian får ett nytt namn; din fil rörs inte.
policy-overwrite = Skriv över
policy-overwrite-description = Ersätt den med den säkerhetskopierade kopian.
policy-skip = Hoppa över
policy-skip-description = Låt den vara och återställ inte den filen.
restore-previewing = Räknar ut vad som kommer att hända…
restore-choose-folder = Välj mappen att återställa till.
restore-preview-failed = Det gick inte att räkna ut vad återställningen skulle göra.
preview-restore = { $count ->
    [one] 1 fil återställs ({ $size })
   *[other] { $count } filer återställs ({ $size })
}
preview-kept = { $count ->
    [one] 1 befintlig fil skiljer sig och behålls bredvid
   *[other] { $count } befintliga filer skiljer sig och behålls bredvid
}
preview-replaced = { $count ->
    [one] 1 befintlig fil skiljer sig och ersätts
   *[other] { $count } befintliga filer skiljer sig och ersätts
}
preview-skipped = { $count ->
    [one] 1 befintlig fil skiljer sig och hoppas över
   *[other] { $count } befintliga filer skiljer sig och hoppas över
}
preview-unchanged = { $count ->
    [one] 1 fil är redan identisk och rörs inte
   *[other] { $count } filer är redan identiska och rörs inte
}
restore-done-title = Återställningen är klar
restore-done-body = { $count ->
    [one] Återställde 1 fil ({ $size }).
   *[other] Återställde { $count } filer ({ $size }).
} { $conflicts ->
    [0] {""}
   *[other] Filer som redan fanns hanterades som du valde.
}
restore-failed = Återställningen slutfördes inte.
browse-failed = Säkerhetskopian kunde inte läsas.
open-copy-failed = Det gick inte att öppna en kopia av filen.
select-scope-folder = Välj en mapp att leta i
select-restore-folder = Välj vart du vill återställa

# Automation
error-password-not-remembered = Schemalagda säkerhetskopieringar behöver lösenordet sparat i nyckelringen. Öppna säkerhetskopian, ange lösenordet med ”Kom ihåg lösenordet” påslaget, så körs nästa schemalagda säkerhetskopiering.
error-keyring-unavailable = Lösenordet ändrades, men kunde inte sparas i nyckelringen: { $details }. Ange det där själv, annars kommer en schemalagd säkerhetskopiering som använder det att misslyckas.
error-delete-unsupported = Stellarshot kan inte radera det här målets egna data på egen hand. Ta bort dem där själv, eller använd Ta bort för att glömma det här utan att radera något.
change = Ändra…
schedule-row = När den körs
check-row = Leta efter skador
check-row-last = Senast kontrollerad { $when }
check-row-never = Aldrig kontrollerad
check-now = Kontrollera nu
check-again = Kontrollera igen
check-failed = Kontrollen slutfördes inte.
check-passed-title = Inga skador hittades
check-passed-body = Varje ögonblicksbild, mapp och indexpost i säkerhetskopian finns och stämmer.
clean-up-row = Frigör utrymme
clean-up-row-description = Glömmer ögonblicksbilder som inställningen ”Behåll” inte längre behöver och tar bort data som ingen ögonblicksbild använder.
clean-up-now = Städa nu
clean-up-failed = Städningen slutfördes inte.
clean-up-cannot-stop = Frigörandet kan inte stoppas när det har börjat.
clean-up-done-title = Städningen är klar
clean-up-done-body = { $count ->
    [one] Glömde 1 ögonblicksbild.
   *[other] Glömde { $count } ögonblicksbilder.
} { $size } behövs inte längre.
progress-cleaning-up = Frigör utrymme…
damaged-title = En kontroll hittade skador i säkerhetskopian
damaged-body = Automatiskt frigörande av utrymme är pausat tills en kontroll godkänns. Ögonblicksbilderna kan fortfarande gå att återställa; för säkerhets skull, starta en ny säkerhetskopia någon annanstans.
failed-just-now = alldeles nyss
failed-minutes-ago = { $count ->
    [one] för en minut sedan
   *[other] för { $count } minuter sedan
}
failed-hours-ago = { $count ->
    [one] för en timme sedan
   *[other] för { $count } timmar sedan
}
failed-days-ago = { $count ->
    [one] i går
   *[other] för { $count } dagar sedan
}
scheduled-backup-failed = Den automatiska säkerhetskopieringen misslyckades ({ $when })
scheduled-cleanup-failed = Städningen efter den automatiska säkerhetskopieringen misslyckades ({ $when })
scheduled-check-failed = Den automatiska kontrollen misslyckades ({ $when })
schedule-failed = Schemat kunde inte ställas in.
schedule-manual = Säkerhetskopierar bara när du trycker på Säkerhetskopiera nu
schedule-hourly = Säkerhetskopierar automatiskt varje timme
schedule-daily = Säkerhetskopierar automatiskt varje dag
schedule-weekly = Säkerhetskopierar automatiskt varje vecka
frequency-hourly = Varje timme
frequency-daily = Varje dag
frequency-weekly = Varje vecka
keep-smart = Smart (rekommenderas)
keep-3-months = Minst 3 månader
keep-6-months = Minst 6 månader
keep-1-year = Minst ett år
keep-days = Minst { $days } dagar
keep-forever = För alltid
keep-smart-description = Behåller den nyaste ögonblicksbilden från var och en av de senaste 7 dagarna som har en, från var och en av de senaste 4 veckorna (måndag till söndag) som har en och från var och en av de senaste 12 kalendermånaderna som har en. Dagar, veckor och månader utan säkerhetskopia hoppas över och räknas inte, och en ögonblicksbild kan vara den som behålls för sin dag, sin vecka och sin månad på en gång. Så länge säkerhetskopiorna sträcker sig över färre än 12 månader behålls även den allra första ögonblicksbilden. Alla andra ögonblicksbilder glöms. Bara den här datorns ögonblicksbilder berörs, och varje uppsättning mappar som säkerhetskopieras räknas för sig.
keep-forever-description = Varje ögonblicksbild behålls. Säkerhetskopian bara växer.
keep-for-description = Varje ögonblicksbild från de { $days } dagarna före den senaste, och äldre allteftersom.
wizard-schedule-title = När ”{ $name }” körs
wizard-when-intro = Säkerhetskopior kan köras av sig själva. Är datorn avstängd eller i vila vid den tiden körs säkerhetskopieringen så snart du är tillbaka.
wizard-automatic = Säkerhetskopiera automatiskt
wizard-automatic-description = Körs i bakgrunden, även när Stellarshot är stängt.
wizard-frequency = Hur ofta
wizard-keep = Gamla ögonblicksbilder
wizard-keep-label = Behåll
wizard-prune = Frigör utrymme automatiskt
wizard-prune-description = Tar bort data som ingen ögonblicksbild längre behöver. Låt det vara av om en annan dator säkerhetskopierar till samma plats.
wizard-append-only = Endast tillägg
wizard-append-only-description = Ett skydd mot misstag, inte attacker: rustic självt vägrar att ta bort en ögonblicksbild från ett endast-tillägg-förråd, men ett verktyg som inte behöver följa det skulle ändå kunna ta bort en direkt. Stellarshot erbjuder inget sätt att stänga av detta igen när det väl är på: Frigör utrymme, och Behåll-inställningen ovan, slutar fungera från den punkten. Välj noga; det finns ingen väg tillbaka från den här skärmen.
wizard-remember-for-schedule = Schemalagda säkerhetskopieringar kan bara köras med sparat lösenord.
notify-backup-failed = Säkerhetskopian ”{ $name }” misslyckades
notify-cleanup-failed = Städningen av ”{ $name }” misslyckades
notify-check-failed = Kontrollen av ”{ $name }” misslyckades
notify-open = Öppna
error-timed-out = Inget svar kom inom { $seconds } sekunder. Anslutningen kan vara långsam, eller så begränsar lagringstjänsten förfrågningarna. Kontrollera anslutningen och försök igen.
place-checking-for = Kontrollerar… { $time }
wizard-creating = Skapar… { $time }
wizard-opening = Öppnar… { $time }
wizard-saving = Sparar…
wizard-creating-note = Förrådet sätts upp. På molnlagring kan det ta en minut.
wizard-opening-note = Säkerhetskopians ögonblicksbilder läses. På molnlagring kan det ta en minut.
progress-uploaded = { $amount } · { $uploaded } sparat
progress-elapsed = Har pågått i { $time }
progress-waiting-preparing = Läser vad säkerhetskopian redan innehåller ({ $time } hittills). På molnlagring kan det ta flera minuter.
progress-waiting = Inget har rört sig på { $time }. Det händer när molnlagringen tar emot data långsamt eller begränsar förfrågningarna; säkerhetskopieringen fortsätter av sig själv.
wizard-estimate-arithmetic = { $included } inkluderat − { $excluded } undantaget = { $total }
wizard-estimate-nothing-excluded = Inget är undantaget från mapparna ovan.
wizard-estimate-adding-up = Räknar… summerar vad undantagen utelämnar.
wizard-patterns-remove = De här mönstren utelämnar { $size } som de undantagna mapparna inte redan utelämnar.
status-up-to-date = Uppdaterad
status-running = Körs
status-overdue = Försenad
status-failed = Misslyckades
status-damaged = Skadad
nav-running = { $name } — körs…
nav-running-percent = { $name } — { $percent } %
menu-help = Hjälp
help = Hjälp
help-icons-title = Vad ikonerna betyder
help-terms-title = Termer
term-repository = Förråd
term-repository-description = Var en säkerhetskopias krypterade, deduplicerade data finns: en mapp, en enhet, en server eller molnlagring. Varje säkerhetskopieprofil har sitt eget.
term-snapshot = Ögonblicksbild
term-snapshot-description = En bild av dina filer från en säkerhetskopiering, tagen vid en viss tidpunkt. Ett förråd innehåller många, och en återställning läser från en av dem.
term-rclone = rclone
term-rclone-description = Det separata program Stellarshot använder för att nå SSH-servrar, Google Drive och annan molnlagring. Det är inte en del av Stellarshot och har sin egen konfiguration.
term-prune = Beskär
term-prune-description = Tar bort data som ingen kvarvarande ögonblicksbild längre behöver, efter att gamla ögonblicksbilder har glömts. "Frigör utrymme automatiskt" gör detta åt dig.
term-keep = Behåll
term-keep-description = Vilka gamla ögonblicksbilder som överlever när utrymme frigörs. "Smart" behåller en krympande historik; "För alltid" behåller allt, och säkerhetskopian bara växer.
next-run = Nästa säkerhetskopiering: { $time }
summary-title = Mappar
summary-none = Inga
summary-included = Inkluderade
summary-excluded = Undantagna
summary-freed = Frigjort genom städning
statistics-title = Förrådsstatistik
statistics-description = Den verkliga storleken i lagringen, komprimeringsgraden och hur mycket som fortfarande kan återvinnas. Läser varje indexfil och listar destinationen.
statistics-calculate = Beräkna
statistics-calculating = Beräknar…
statistics-stored = Lagrat på destinationen
statistics-ratio = Komprimeringsgrad
statistics-no-ratio = Inte känt än
statistics-reclaimable = Kan återvinnas genom städning
history-title = Historik
event-backed-up = Säkerhetskopierad
event-stage-backup = Säkerhetskopiering
event-stage-check = Kontroll
event-stage-cleanup = Städning
event-failed = { $stage } misslyckades: { $reason }
event-skipped = Hoppades över: { $reason }
event-checked-sound = Kontrollen godkändes
event-checked-damaged = Kontrollen hittade skador
event-cleaned-up = Glömde { $count } ögonblicksbilder, frigjorde { $size }
notify-overdue = "{ $name }" har inte säkerhetskopierats på ett tag
notify-overdue-body = Dess destination har inte varit nåbar vid de schemalagda tiderna. { $schedule } Kontrollera att den är ansluten, öppna sedan Stellarshot för att säkerhetskopiera nu.
settings-backup-title = Säkerhetskopiera och återställ Stellarshots egna inställningar
settings-export = Exportera inställningar
settings-export-description = Varje säkerhetskopias mappar, destination och schema, samt dess historik. Aldrig ett lösenord.
settings-export-button = Exportera…
settings-export-title = Spara Stellarshots inställningar
settings-export-done-title = Inställningar exporterade
settings-export-done-body = Varje säkerhetskopias inställningar och historik sparades i filen du valde.
settings-export-failed = Inställningarna kunde inte sparas.
settings-import = Importera inställningar
settings-import-description = Lägg till säkerhetskopior från en inställningsexport. En som redan finns här lämnas precis som den är; bara dess historik läggs till.
settings-import-button = Importera…
settings-import-title = Välj en inställningsexport att importera
settings-import-done-title = Inställningar importerade
settings-import-done-body = { $added ->
    [0] Inga nya säkerhetskopior lades till.
    [1] En säkerhetskopia lades till.
   *[other] { $added } säkerhetskopior lades till.
} { $skipped ->
    [0] {""}
    [1] En fanns redan här och lämnades som den är.
   *[other] { $skipped } fanns redan här och lämnades som de är.
}
settings-import-failed = Inställningarna kunde inte importeras.
home = Översikt
home-backups-title = Säkerhetskopior
home-backup-detail = { $status } · { $last }
home-view = Visa
home-folders-title = Mappar som säkerhetskopieras på den här datorn
home-locations-title = Lagringsplatser
home-location-detail = { $kind } · { $backups }
wizard-resume = Återuppta inställning
wizard-cancel-title = Avbryta inställningen av den här säkerhetskopian?
wizard-cancel-body = Du kan återkomma till den senare från sidopanelen, eller kasta allt som skrivits hittills.
wizard-finish-later = Slutför senare
wizard-discard = Kasta
place-google-advanced = Använd mina egna Google API-uppgifter…
place-google-advanced-description = Logga in med en egen Google Cloud-klient istället för den rclone delar med alla som inte har satt upp en egen. Behöver både ett klient-ID och en klienthemlighet från ditt eget Google Cloud-projekt; lämna båda tomma för att använda den delade standarden.
place-google-client-id = Klient-ID
place-google-client-secret = Klienthemlighet
place-advanced = Avancerat
place-bandwidth-limit = Bandbreddsgräns
place-bandwidth-limit-description = Begränsar hur snabbt den här säkerhetskopian laddar upp och ner, i rclones egen syntax (1M, eller 8M:2M för uppladdning:nedladdning). Tomt för ingen gräns.
place-bandwidth-limit-placeholder = t.ex. 1M
wizard-exclude-caches = Utelämna cachemappar
wizard-exclude-caches-description = Hoppa över mappar som märker sig själva som utbytbar cachedata med en CACHEDIR.TAG-fil.
wizard-git-ignore = Följ .gitignore
wizard-git-ignore-description = Utelämna allt som varje projekts egen .gitignore redan utesluter.
wizard-skip-if-unchanged = Hoppa över tomma säkerhetskopior
wizard-skip-if-unchanged-description = Spela inte in en ny ögonblicksbild när inget har ändrats sedan den senaste.
restore-advanced = Avancerat
restore-verify-existing = Verifiera befintliga filer
restore-verify-existing-description = Läs och kontrollera en fil som redan verkar oförändrad, i stället för att lita på dess storlek och ändringstid.
restore-ownership-preserve = Återställ den ursprungliga ägaren
restore-ownership-numeric = Återställ numeriska användar- och grupp-ID:n
restore-ownership-none = Återställ inte ägarskap
settings-cache-title = Lokal cache
settings-cache-dir = Cacheplats
settings-cache-dir-default = Standard (~/.cache/rustic)
settings-cache-dir-choose = Välj…
settings-cache-dir-reset = Använd standard
settings-cache-dir-title = Välj en cachemapp
settings-no-cache = Använd ingen cache alls
settings-no-cache-description = Långsammare, men inget värt att spara på en dator med lite diskutrymme.
settings-global-excludes-title = Utelämnat från varje säkerhetskopia
settings-global-excludes-description = Globmönster som node_modules eller target, tillämpade på varje säkerhetskopia utan att lägga till dem i var och en.
pin-snapshot-failed = Ögonblicksbildens fästning kunde inte ändras.
pin-snapshot = Fäst, så att uppstädning aldrig tar bort den här ögonblicksbilden
unpin-snapshot = Lossa, så att uppstädning kan ta bort den här ögonblicksbilden igen
delete-snapshot-row = Ta bort den här ögonblicksbilden
