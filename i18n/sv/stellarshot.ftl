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
