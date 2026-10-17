# Скрипты Базиса (BAZIS-Script)

Движок — boa (JavaScript на чистом Rust), крейт `crates/script`. Объекты Базиса написаны на JavaScript в
`crates/script/src/prelude.js` поверх одного моста к сессии проекта. Поэтому скрипт меняет модель только
командами программы, и каждое изменение проверяется. При ошибке скрипта модель не меняется. Все изменения
скрипта отменяются одной отменой.

## Где запускать

- **Окно:** «Скрипты → Выполнить скрипт Базиса…».
  - Окна скрипта (`NewForm`, `Action.Properties`) показываются живыми: кнопки и поля вызывают обработчики скрипта.
  - «Применить» переносит изменения в модель.
- **CLI:** `basis-craft script проект.bcraft скрипт.js`.
- **MCP:** инструмент `run_script`.

## Что есть

- **Старое API:**
  - модель: `Model` (`forEachPanel`, `forEach`, `Selections`, `Objects`, `Count`);
  - создание панелей: `AddFrontPanel`, `AddHorizPanel`, `AddVertPanel`;
  - контур: `NewContour` (`AddLine`, `AddArc`, `AddArc3`, `AddCircle`, `AddRectangle`, `IsPointInside`, `IsContourRectangle`, `FindNearestObject`, `[i]`), элементы `T2DLine`/`T2DArc`/`T2DCircle`;
  - действия: `Action` (`Properties`, `OnFinish`, `Continue`/`Finish`/`Commit`), `Undo`;
  - окна: `NewForm` и элементы окна;
  - сообщения: `MessageBox`, `alert`, `prompt`, `confirm`;
  - файлы и система: `system`, `fs`, `path`, `os`, `require`.
- **Панель (`TFurnPanel`/`TObject3D`):**
  - имя, позиция, обозначение, материал, толщина, текстура;
  - контур с кромкой (`Data.Butt`), `Butts`, отверстия;
  - габариты (`GSize`, `GabMin`/`GabMax`), положение, переводы координат;
  - сдвиг, поворот вокруг осей проекта, копирование, удаление, `AddButt`.
- **Модульное API:** `objects3d.NewPanel`, `panelOperations.AddButt`, `materialData` (активные материал и кромка), `geometry2d`/`geometry3d`, `historyOperations`, `execution`, `batchProcessing`, `objectTypeChecker`, `currentFileData`, `apiVersion`.
- **Файлы:** только в папке скрипта; путь с «..» отклоняется.

## Прогон скриптов пользователя

Тест `crates/script/tests/reference.rs` запускает все 270 скриптов из `reference/scripts` на «Кухне 24.04» без окна.

- **Без ошибок: 81 из 270 (30 %).**
- «Без ошибок» не означает «сделал ровно то же, что Базис». Скрипты с окнами без окна ждут ответа по умолчанию.
- Нижняя граница — 81. Тест не даёт ей опуститься.

### Причины отказов (189 скриптов)

| Скриптов | Причина |
|---|---|
| 106 | Член объекта ещё не сделан (обычно редкий метод панели, контура или окна) |
| 35 | Функция старого API ещё не сделана (`TFurnBlock`, `AddLayer`, `NewFloatInput`…) |
| 14 | Windows / Node.js: COM, `zlib`, `child_process`, `secureExec` |
| 10 | Запутанный (обфусцированный) код с защитой от изменения среды |
| 8 | Крепёж как объект Базиса (`fastenerOperations`, `TFastener`) |
| 8 | Объекты размеров (`Size3D`) |
| 3 | Эластичность |
| 3 | Таймеры (`setInterval`) |
| 2 | Синтаксис |

Чего нет в программе, то отвечает ошибкой с именем (`elasticTransformation.ResizeObject: недоступно в basis-craft`), а не делает вид, что сработало.
