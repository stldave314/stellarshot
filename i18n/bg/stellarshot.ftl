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
place-rclone-missing = За копиране тук е нужен rclone. Инсталирайте го (например със sudo apt install rclone) и опитайте отново.
place-checking = Проверка…
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
