# Application
stellarshot = Stellarshot

# Empty state
empty-title = Пазете файловете си
empty-body = Правете резервни копия на папките си на друго устройство или в друга папка. Копията са шифровани, а след първото се записват само промените.
create-backup = Създаване на резервно копие…
open-existing = Отваряне на съществуващо резервно копие

# Profile page
never-backed-up = Все още няма резервно копие
backed-up-just-now = Последно резервно копие току-що
backed-up-minutes-ago = Последно резервно копие преди { $count ->
    [one] една минута
   *[other] { $count } минути
}
backed-up-hours-ago = Последно резервно копие преди { $count ->
    [one] един час
   *[other] { $count } часа
}
backed-up-days-ago = Последно резервно копие преди { $count ->
    [one] един ден
   *[other] { $count } дни
}
status-detail = { $destination } · { $count ->
    [one] едно моментно състояние
   *[other] { $count } моментни състояния
}
back-up-now = Резервно копие сега
choose-what-title = Изберете какво да се копира
choose-what-body = За това резервно копие все още няма избрани папки.
choose-what-button = Избиране на папки…
unlock-title = Въведете паролата за това резервно копие
unlock = Отключване
remember-password = Запомняне на паролата
remember-password-description = Пази се в ключодържателя ви. Планираните резервни копия се нуждаят от нея.
recent-snapshots = Последни моментни състояния
no-snapshots-yet = Все още няма моментни състояния. Натиснете „Резервно копие сега“ за първото.
snapshot-row = { $id } · { $size } · { $added } нови
show-all-snapshots = Показване на всички { $count } моментни състояния
manage = Управление
edit-backup = Какво се копира
edit-backup-description = Промяна на включените и изключените папки.
change-password-row = Парола
change-password-row-description = Промяна на паролата, с която това резервно копие е отключено.
change-password-title = Промяна на паролата
change-password-body = Добавя ключ за новата парола, след което премахва този, с който сте отключени сега. Ключът на друг човек или на лист за възстановяване остава непроменен.
change-password-command-note = Ще трябва да я обновите и навсякъде другаде, където е съхранена текущата парола — това променя само самия архив.
change-password-failed = Паролата не можа да бъде променена.
password-source-row = Как се предоставя паролата
password-source-keyring = Съхранена в ключодържателя
password-source-command = От команда
password-source-title = Как се предоставя паролата
password-source-body = Команда, която извежда паролата на стандартния си изход, изпълнявана наново всеки път, когато е нужна, вместо ключодържателя — за мениджър на пароли с клиент за командния ред, например Bitwarden CLI. Оставете празно, за да се използва ключодържателят.
password-source-placeholder = напр. bw get password stellarshot-home
password-command-failed = Командата за паролата не можа да се изпълни.
remove-backup = Премахване от Stellarshot
remove-backup-description = Stellarshot забравя това резервно копие. Самото копие остава на мястото си.
delete-backup = Изтриване на копието и всички данни
delete-backup-description = Изтрива окончателно всяко моментно състояние в това копие.
progress-starting = Започване…
progress-preparing = Подготовка…
progress-backing-up = Създаване на резервно копие…
progress-restoring = Възстановяване…
progress-checking = Проверка…
progress-amount = { $done } от { $total }

# Wizard
wizard-create-title = Ново резервно копие
wizard-open-title = Отваряне на съществуващо резервно копие
wizard-edit-title = Какво копира „{ $name }“
wizard-step = Стъпка { $current } от { $total }
wizard-what-intro = Изберете папките за копиране и какво в тях да се пропусне.
wizard-include = Включване
wizard-exclude = Изключване
wizard-exclude-outside = Не е във включена папка, затова не променя нищо
wizard-add-folders = Добавяне на папки…
wizard-pick-sources = Изберете папки за копиране
wizard-pick-excludes = Изберете папки за пропускане
wizard-advanced = Разширени
wizard-pattern-placeholder = Пропускане на съвпадащи имена, напр. *.tmp или node_modules
wizard-one-file-system = Оставане на същото устройство
wizard-one-file-system-description = Без преминаване в други устройства или мрежови ресурси, монтирани в тези папки.
wizard-estimate-label = Приблизителен размер
wizard-estimate = { $size } · { $files } файла
wizard-estimate-counting = Преброяване…
wizard-estimate-note = Първото копие обикновено е по-малко след компресия и премахване на дубликати. Следващите записват само промените.
wizard-where-intro = Изберете къде да се пази копието: празна папка, за предпочитане на друго устройство.
wizard-where-title = Местоположение
wizard-no-folder = Няма избрана папка
wizard-choose-folder = Избиране на папка…
select-repo-folder = Изберете папка за хранилището
wizard-where-new = Тук ще бъде създадено ново резервно копие.
wizard-where-existing = Тази папка вече съдържа резервно копие. Използвайте „Отваряне на съществуващо резервно копие“, за да го добавите.
wizard-where-found = Тук беше намерено резервно копие.
wizard-where-no-repository = В тази папка няма резервно копие.
wizard-where-not-empty = Тази папка вече съдържа други файлове. Изберете празна папка.
wizard-name = Име
wizard-name-placeholder = Например: Домашна папка на USB устройство
wizard-secure-intro = Изберете парола. Копието ви се шифрова с нея.
wizard-open-intro = Въведете паролата, с която е създадено това копие.
wizard-confirm = Потвърждаване на паролата
wizard-mismatch = Паролите не съвпадат.
wizard-password-warning = Ако загубите тази парола, резервните ви копия не могат да бъдат възстановени. Никой не може да я възстанови вместо вас.
wizard-finish-create = Създаване и копиране сега
wizard-finish-open = Отваряне

# Dialogs and buttons
ok = Добре
save = Запазване
add = Добавяне
back = Назад
next = Напред
edit = Редактиране
remove = Премахване
delete = Изтриване
cancel = Отказване
password = Парола
remove-title = Премахване на „{ $name }“?
remove-body = Stellarshot ще забрави това копие и запазената му парола. Копието и моментните му състояния не се изтриват и могат да бъдат отворени отново.
delete-title = Изтриване на „{ $name }“ и всички данни?
delete-body = Това изтрива окончателно копието и всяко моментно състояние в него. Другите файлове в същата папка не се засягат. Въведете { $name } за потвърждение.

# Errors
error-title = Нещо се обърка
error-details = Подробности: { $details }
location-not-empty = { $path } вече съдържа други файлове. Изберете празна папка или папка, която вече съдържа хранилище.
create-repo-failed = Хранилището не можа да бъде създадено.
delete-repo-failed = Хранилището не можа да бъде изтрито.
delete-snapshot-failed = Моментното състояние не можа да бъде изтрито.
snapshot-failed = Моментното състояние не можа да бъде създадено.
open-repo-failed = Хранилището не можа да бъде отворено.
error-wrong-password = Паролата е грешна.
error-not-a-repository = В { $path } няма хранилище.
error-already-exists = { $path } вече съдържа хранилище.
error-destination-unavailable = { $path } е недостъпно. Ако е на преносимо устройство или мрежов ресурс, проверете дали е свързано.
error-locked = Друго резервно копиране вече използва това хранилище. Опитайте отново, когато приключи.
error-cancelled = Операцията беше отменена. Нищо не беше променено.
error-repository-damaged = Проверката на хранилището откри проблеми. Не изтривайте други копия на данните си, докато това не бъде решено.

# About
about = Относно
about-author = Сътрудниците на Stellarshot
about-credits = Базирано на Stellarshot от проекта cosmic-utils.
repository = Хранилище
support = Поддръжка

# Settings
settings = Настройки
appearance = Външен вид
theme = Тема
match-desktop = Системен
dark = Тъмна
light = Светла

# Menu
file = Файл
menu-new-backup = Ново резервно копие…
new-backup = Ново резервно копие
new-window = Нов прозорец
quit = Спиране на програмата
view = Изглед
menu-settings = Настройки...
menu-about = Относно „Stellarshot“...

# Storage locations
google-drive = Google Drive
place-folder = Папка
place-folder-description = Папка на този компютър или на монтирано устройство
place-drive = Преносимо устройство
place-drive-description = USB устройство, където и да е монтирано
place-server = Мрежов сървър (SFTP)
place-server-description = Папка на компютър, достъпен през SSH
place-google-description = Вашият профил в Google, вписан от Stellarshot
place-remote = Един от вашите rclone ресурси
place-remote-description = OneDrive, Dropbox, S3 и всичко друго, до което rclone достига
place-rest = REST сървър
place-rest-description = Собствен rest-server или rustic-server
place-rest-url = URL на сървъра
place-rest-url-placeholder = http://user:pass@host:8000/repo/
place-rclone-missing = За копиране тук е нужен rclone. Инсталирайте го (например със sudo apt install rclone) и опитайте отново.
place-check-failed = Мястото не можа да бъде проверено.
place-check = Проверка
place-no-drives = Няма свързани преносими устройства. Свържете едно, после се върнете назад и напред.
place-folder-on-drive = Папка на устройството
place-host = Сървър
place-user = Потребителско име
place-user-placeholder = Вашето потребителско име на този компютър
place-port = Порт
place-server-path = Папка на сървъра
place-server-note = Удостоверяването използва вашия SSH агент или ключове, а сървърът трябва вече да е в ~/.ssh/known_hosts: свържете се веднъж със ssh.
place-signing-in = Завършете вписването в браузъра. Stellarshot чака…
place-google-intro = Stellarshot ще отвори браузъра, за да се впишете в Google. Вписването се пази само в собствените настройки на Stellarshot.
place-sign-in = Вписване с Google…
place-signed-in = Вписано.
place-cloud-folder = Папка
place-no-remotes = Нямате rclone ресурси. Създайте такъв с rclone config и се върнете.
error-rclone-missing = rclone не е инсталиран. Инсталирайте го (например със sudo apt install rclone), за да използвате това място.
error-auth-failed = Вписването не завърши: { $details }

# Déjà Dup import
dejadup-import = Внасяне от Déjà Dup
dejadup-title = Внасяне на резервно копие от Déjà Dup
dejadup-name = Резервно копие от Déjà Dup
dejadup-none = Не са намерени настройки на Déjà Dup.
dejadup-other-format = Това копие от Déjà Dup използва по-стария формат duplicity, който Stellarshot не може да чете. Създайте ново копие; старото остава четимо в Déjà Dup.
dejadup-unsupported = Déjà Dup пази това копие на място, което Stellarshot не може да използва ({ $backend }). Създайте ново копие.
menu-import-dejadup = Внасяне от Déjà Dup…

# Restore
restore-open = Възстановяване…
restore-title = Възстановяване от { $name }
restore-loading = Отваряне на копието…
tab-browse = Преглед
tab-deleted = Изтрити файлове
tab-compare = Сравняване
selected-count = { $count ->
    [one] 1 избран елемент
   *[other] { $count } избрани елемента
}
restore-button = Възстановяване…
search-placeholder = Търсене в това моментно състояние
search-results = { $count ->
    [one] 1 съвпадение
   *[other] { $count } съвпадения
}
folder-empty = Тази папка е празна.
versions-title = Версии
versions-same = { $count ->
    [one] ↳ същото в 1 по-старо състояние
   *[other] ↳ същото в { $count } по-стари състояния
}
open-copy = Отваряне на копие
restore-this-version = Възстановяване на тази версия…
deleted-scope = Файлове в { $folder }, които са в копия от последните { $days } дни, но вече ги няма на диска.
deleted-change-folder = Смяна на папката…
deleted-find = Търсене на изтрити файлове
restore-searching = Търсене…
deleted-intro = Потърсете изтрити файлове, които копие все още пази.
deleted-none = Нищо не липсва: всеки файл от тези копия още е на диска.
deleted-last-seen = последно копиран { $when }
compare-button = Сравняване
compare-intro = Изберете две моментни състояния, за да видите промените.
compare-none = Нищо не се е променило между тези състояния.
compare-summary = { $added } добавени · { $removed } премахнати · { $changed } променени
restore-sheet-title = { $count ->
    [one] Възстановяване на 1 елемент
   *[other] Възстановяване на { $count } елемента
}
restore-to = Възстановяване в
restore-to-original = Където са били
restore-to-folder = Друга папка…
restore-to-folder-chosen = В { $folder }
restore-existing = Ако файлът вече съществува
policy-keep-both = Запазване на двата
policy-keep-both-description = Възстановеното копие получава ново име; вашият файл не се пипа.
policy-overwrite = Презаписване
policy-overwrite-description = Замяна с копието от архива.
policy-skip = Пропускане
policy-skip-description = Оставяне и без възстановяване на този файл.
restore-previewing = Изчисляване какво ще стане…
restore-choose-folder = Изберете папка за възстановяване.
restore-preview-failed = Не можа да се изчисли какво би направило възстановяването.
preview-restore = { $count ->
    [one] 1 файл ще бъде възстановен ({ $size })
   *[other] { $count } файла ще бъдат възстановени ({ $size })
}
preview-kept = { $count ->
    [one] 1 съществуващ файл се различава и ще бъде запазен до копието
   *[other] { $count } съществуващи файла се различават и ще бъдат запазени до копията
}
preview-replaced = { $count ->
    [one] 1 съществуващ файл се различава и ще бъде заменен
   *[other] { $count } съществуващи файла се различават и ще бъдат заменени
}
preview-skipped = { $count ->
    [one] 1 съществуващ файл се различава и ще бъде пропуснат
   *[other] { $count } съществуващи файла се различават и ще бъдат пропуснати
}
preview-unchanged = { $count ->
    [one] 1 файл вече е идентичен и няма да бъде пипан
   *[other] { $count } файла вече са идентични и няма да бъдат пипани
}
restore-done-title = Възстановяването приключи
restore-done-body = { $count ->
    [one] Възстановен е 1 файл ({ $size }).
   *[other] Възстановени са { $count } файла ({ $size }).
} { $conflicts ->
    [0] {""}
   *[other] Съществуващите файлове са обработени според избора ви.
}
restore-failed = Възстановяването не завърши.
browse-failed = Това копие не можа да бъде прочетено.
open-copy-failed = Не можа да се отвори копие на този файл.
select-scope-folder = Изберете папка за търсене
select-restore-folder = Изберете къде да се възстанови

# Automation
error-password-not-remembered = Планираните архивирания изискват паролата да е запомнена в ключодържателя. Отворете архива, въведете паролата му с включено „Запомняне на паролата“ и следващото планирано архивиране ще се изпълни.
error-keyring-unavailable = Паролата беше променена, но не можа да бъде запазена в ключодържателя: { $details }. Въведете я там сами, иначе планирано архивиране с нея ще се провали.
error-delete-unsupported = Stellarshot не може да изтрие данните на тази дестинация сам. Премахнете ги там сами или използвайте „Премахване“, за да ги забравите тук, без да изтривате нищо.
change = Промяна…
schedule-row = Кога се изпълнява
check-row = Проверка за повреди
check-row-last = Последна проверка { $when }
check-row-never = Никога не е проверявано
check-now = Проверка сега
check-again = Нова проверка
check-failed = Проверката не завърши.
check-passed-title = Не са открити повреди
check-passed-body = Всяко моментно състояние, папка и запис в индекса на този архив е налице и е съгласувано.
clean-up-row = Освобождаване на място
clean-up-row-description = Забравя моментните състояния, които настройката „Пазене“ вече не изисква, и изтрива данни, които не се използват.
clean-up-now = Почистване сега
clean-up-failed = Почистването не завърши.
clean-up-cannot-stop = Освобождаването на място не може да бъде спряно, след като започне.
clean-up-done-title = Почистването приключи
clean-up-done-body = { $count ->
    [one] Забравено е 1 моментно състояние.
   *[other] Забравени са { $count } моментни състояния.
} { $size } вече не са нужни.
progress-cleaning-up = Освобождаване на място…
damaged-title = Проверка откри повреда в този архив
damaged-body = Автоматичното освобождаване на място е спряно, докато проверка не премине успешно. Моментните състояния може все още да се възстановяват; за всеки случай започнете нов архив на друго място.
failed-just-now = току-що
failed-minutes-ago = { $count ->
    [one] преди минута
   *[other] преди { $count } минути
}
failed-hours-ago = { $count ->
    [one] преди час
   *[other] преди { $count } часа
}
failed-days-ago = { $count ->
    [one] вчера
   *[other] преди { $count } дни
}
scheduled-backup-failed = Автоматичното архивиране не успя ({ $when })
scheduled-cleanup-failed = Почистването след автоматичното архивиране не успя ({ $when })
scheduled-check-failed = Автоматичната проверка не успя ({ $when })
schedule-failed = Графикът не можа да бъде настроен.
schedule-manual = Архивира само когато натиснете „Резервно копие сега“
schedule-hourly = Архивира автоматично всеки час
schedule-daily = Архивира автоматично всеки ден
schedule-weekly = Архивира автоматично всяка седмица
frequency-hourly = Всеки час
frequency-daily = Всеки ден
frequency-weekly = Всяка седмица
keep-smart = Интелигентно (препоръчително)
keep-3-months = Поне 3 месеца
keep-6-months = Поне 6 месеца
keep-1-year = Поне година
keep-days = Поне { $days } дни
keep-forever = Завинаги
keep-smart-description = Пази най-новото моментно състояние от всеки от последните 7 дни, в които има такова, от всяка от последните 4 седмици (от понеделник до неделя), в които има такова, и от всеки от последните 12 календарни месеца, в които има такова. Дни, седмици и месеци без резервно копие се пропускат, а не се броят, и едно моментно състояние може едновременно да е запазеното за своя ден, своята седмица и своя месец. Докато копията обхващат по-малко от 12 месеца, се пази и най-първото моментно състояние. Всички останали моментни състояния се забравят. Засягат се само моментните състояния на този компютър и всеки набор от архивирани папки се брои отделно.
keep-forever-description = Всяко моментно състояние се пази. Архивът само расте.
keep-for-description = Всяко моментно състояние от { $days } дни преди най-новото, а по-старите постепенно отпадат.
wizard-schedule-title = Кога се изпълнява „{ $name }“
wizard-when-intro = Архивирането може да се изпълнява само. Ако компютърът е изключен или заспал по това време, архивирането се изпълнява веднага щом се върнете.
wizard-automatic = Автоматично архивиране
wizard-automatic-description = Изпълнява се във фонов режим, дори когато Stellarshot е затворен.
wizard-frequency = Колко често
wizard-keep = Стари моментни състояния
wizard-keep-label = Пазене
wizard-prune = Автоматично освобождаване на място
wizard-prune-description = Изтрива данни, които вече не са нужни. Оставете изключено, ако друг компютър архивира на същото място.
wizard-append-only = Само добавяне
wizard-append-only-description = Защита срещу грешки, не срещу атаки: самата rustic отказва да изтрие моментно състояние от хранилище само за добавяне, но инструмент, който не е длъжен да го спазва, все пак би могъл да премахне такова направо. Stellarshot не предлага начин това да се изключи отново, щом бъде включено: „Освобождаване на място“ и настройката „Запазване“ по-горе спират да работят от този момент нататък. Изберете внимателно; от този екран няма връщане назад.
wizard-remember-for-schedule = Планираните архивирания могат да се изпълняват само със запомнена парола.
notify-backup-failed = Архивирането „{ $name }“ не успя
notify-cleanup-failed = Почистването на „{ $name }“ не успя
notify-check-failed = Проверката на „{ $name }“ не успя
notify-open = Отваряне
error-timed-out = Нямаше отговор в рамките на { $seconds } секунди. Връзката може да е бавна или услугата за съхранение да ограничава заявките. Проверете връзката и опитайте отново.
place-checking-for = Проверка… { $time }
wizard-creating = Създаване… { $time }
wizard-opening = Отваряне… { $time }
wizard-saving = Запазване…
wizard-creating-note = Хранилището се подготвя. В облачно хранилище това може да отнеме минута.
wizard-opening-note = Моментните състояния на копието се прочитат. В облачно хранилище това може да отнеме минута.
progress-uploaded = { $amount } · { $uploaded } записани
progress-elapsed = Работи от { $time }
progress-waiting-preparing = Прочита се какво вече съдържа копието (досега { $time }). В облачно хранилище това може да отнеме няколко минути.
progress-waiting = Нищо не се е променило от { $time }. Това се случва, когато облачното хранилище приема данни бавно или ограничава заявките; копирането продължава само.
wizard-estimate-arithmetic = { $included } включени − { $excluded } изключени = { $total }
wizard-estimate-nothing-excluded = Нищо не е изключено от папките по-горе.
wizard-estimate-adding-up = Преброяване… сумира се какво изпускат изключенията.
wizard-patterns-remove = Тези шаблони изпускат { $size } извън изключените папки.
status-up-to-date = Актуално
status-running = Изпълнява се
status-overdue = Просрочено
status-failed = Неуспешно
status-damaged = Повредено
nav-running = { $name } — изпълнява се…
nav-running-percent = { $name } — { $percent }%
menu-help = Помощ
help = Помощ
help-icons-title = Какво означават иконите
help-terms-title = Термини
term-repository = Хранилище
term-repository-description = Къде живеят криптираните, дедуплицирани данни на резервно копие: папка, устройство, сървър или облачно хранилище. Всеки профил за резервно копие има свое собствено.
term-snapshot = Моментно състояние
term-snapshot-description = Картина на файловете ви от едно архивиране, направена в определен момент. Едно хранилище пази много такива, а възстановяването чете от едно от тях.
term-rclone = rclone
term-rclone-description = Отделната програма, чрез която Stellarshot достига SSH сървъри, Google Drive и друго облачно хранилище. Тя не е част от Stellarshot и пази собствена конфигурация.
term-prune = Прочистване
term-prune-description = Изтрива данните, които вече не са нужни на нито едно оставащо моментно състояние, след като старите моментни състояния са забравени. „Автоматично освобождаване на пространство“ прави това вместо вас.
term-keep = Пазене
term-keep-description = Кои стари моментни състояния оцеляват при освобождаване на пространство. „Умно“ пази смаляваща се история; „Завинаги“ пази всичко, а копието само расте.
next-run = Следващо архивиране: { $time }
summary-title = Папки
summary-none = Няма
summary-included = Включени
summary-excluded = Изключени
summary-freed = Освободено от почиствания
statistics-title = Статистика на хранилището
statistics-description = Реалният размер в хранилището, степента на компресия и колко още може да бъде освободено. Прочита всеки индексен файл и изброява местоназначението.
statistics-calculate = Изчисляване
statistics-calculating = Изчисляване…
statistics-stored = Съхранено в местоназначението
statistics-ratio = Степен на компресия
statistics-no-ratio = Все още не е известно
statistics-reclaimable = Може да бъде освободено при почистване
history-title = История
event-backed-up = Архивирано
event-stage-backup = Архивиране
event-stage-check = Проверка
event-stage-cleanup = Почистване
event-failed = { $stage } неуспешно: { $reason }
event-skipped = Пропуснато: { $reason }
event-checked-sound = Проверката премина успешно
event-checked-damaged = Проверката откри повреда
event-cleaned-up = Забравени { $count } моментни състояния, освободени { $size }
notify-overdue = „{ $name }“ не е архивирано отдавна
notify-overdue-body = Местоназначението му не е било достъпно в планираните часове. { $schedule } Проверете дали е свързано, после отворете Stellarshot, за да архивирате сега.
settings-backup-title = Архивиране и възстановяване на собствените настройки на Stellarshot
settings-export = Изнасяне на настройки
settings-export-description = Папките, местоназначението и графикът на всяко резервно копие, както и историята му. Никога парола.
settings-export-button = Изнасяне…
settings-export-title = Запазване на настройките на Stellarshot
settings-export-done-title = Настройките са изнесени
settings-export-done-body = Настройките и историята на всяко резервно копие бяха запазени в избрания файл.
settings-export-failed = Настройките не можаха да бъдат запазени.
settings-import = Внасяне на настройки
settings-import-description = Добавяне на резервни копия от изнесени настройки. Копие, което вече е тук, се оставя точно както е; добавя се само историята му.
settings-import-button = Внасяне…
settings-import-title = Изберете изнесени настройки за внасяне
settings-import-done-title = Настройките са внесени
settings-import-done-body = { $added ->
    [0] Не бяха добавени нови резервни копия.
    [1] Беше добавено едно резервно копие.
   *[other] Бяха добавени { $added } резервни копия.
} { $skipped ->
    [0] {""}
    [1] Едно вече беше тук и беше оставено непроменено.
   *[other] { $skipped } вече бяха тук и бяха оставени непроменени.
}
settings-import-failed = Настройките не можаха да бъдат внесени.
home = Общ преглед
home-backups-title = Резервни копия
home-backup-detail = { $status } · { $last }
home-view = Преглед
home-folders-title = Папки, архивирани на този компютър
home-locations-title = Места за съхранение
home-location-detail = { $kind } · { $backups }
wizard-resume = Продължаване на настройката
wizard-cancel-title = Спиране на настройката на това резервно копие?
wizard-cancel-body = Можете да се върнете към нея по-късно от страничния панел, или да отхвърлите всичко въведено дотук.
wizard-finish-later = Довършване по-късно
wizard-discard = Отхвърляне
place-google-advanced = Използване на собствени идентификационни данни за Google API…
place-google-advanced-description = Впишете се със собствен клиент на Google Cloud, вместо със споделения от rclone с всички, които нямат собствен. Нужни са и клиентски идентификатор, и клиентска тайна от собствен проект в Google Cloud; оставете и двете празни, за да използвате споделения по подразбиране.
place-google-client-id = Клиентски идентификатор
place-google-client-secret = Клиентска тайна
place-advanced = Разширени
place-bandwidth-limit = Ограничение на честотната лента
place-bandwidth-limit-description = Ограничава колко бързо това резервно копие качва и изтегля, в собствения синтаксис на rclone (1M, или 8M:2M за качване:изтегляне). Празно за без ограничение.
place-bandwidth-limit-placeholder = напр. 1M
wizard-exclude-caches = Пропускане на папки за кеш
wizard-exclude-caches-description = Пропускане на всяка папка, която се обозначава като временни кеш данни с файл CACHEDIR.TAG.
wizard-git-ignore = Спазване на .gitignore
wizard-git-ignore-description = Пропускане на всичко, което собственият файл .gitignore на всеки проект вече изключва.
wizard-skip-if-unchanged = Пропускане на празни резервни копия
wizard-skip-if-unchanged-description = Без записване на ново моментно състояние, когато нищо не се е променило от последното.
restore-advanced = Разширени
restore-verify-existing = Проверка на съществуващите файлове
restore-verify-existing-description = Четене и проверка на файл, който вече изглежда непроменен, вместо да се доверявате на неговия размер и време на промяна.
restore-ownership-preserve = Възстановяване на оригиналния собственик
restore-ownership-numeric = Възстановяване на числови идентификатори на потребител и група
restore-ownership-none = Без възстановяване на собствеността
settings-cache-title = Локален кеш
settings-cache-dir = Местоположение на кеша
settings-cache-dir-default = По подразбиране (~/.cache/rustic)
settings-cache-dir-choose = Избор…
settings-cache-dir-reset = Използване по подразбиране
settings-cache-dir-title = Избор на папка за кеша
settings-no-cache = Без кеширане изобщо
settings-no-cache-description = По-бавно, но няма какво да се пази на машина с малко дисково пространство.
settings-global-excludes-title = Пропуснато във всяко резервно копие
settings-global-excludes-description = Глоб шаблони като node_modules или target, прилагани към всяко резервно копие, без да се добавят към всяко поотделно.
pin-snapshot-failed = Закачването на моментното състояние не можа да бъде променено.
pin-snapshot = Закачане, за да не се премахва това моментно състояние при почистване
unpin-snapshot = Откачане, за да може почистването отново да премахне това моментно състояние
