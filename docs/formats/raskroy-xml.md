# Проект БАЗИС-Раскроя (XML)

Эталоны: `reference/raskroy-project-*.xml`. Кодировка windows-1251, строки CRLF. Реализация: `crates/basis/src/raskroy.rs`.

```
PROJECT version=2 subver=7 name=… code=""
  MATERIALROOTNODE
    COREBOARDMAT                       — плитные материалы; SHEETDATA/RECTANGLE: формат листа
    EDGEBOARDMAT                       — кромка
  BOARDROOTNODE
    SMPBOARDS/BOARDSMP*                — прямоугольные детали
    FIGBOARDS/BOARDFIG*                — фигурные детали
    BENTBOARDS                         — гнутые детали
  BINDINGROOTNODE                      — крепёж и фурнитура (не выгружаем)
```

## Правила, установленные по эталонам

- **Количество.** Каждая деталь выгружается отдельно, `QTY=1`.
- **Размеры.**
  - `SIZEX`/`SIZEY` — габарит контура в его собственных осях.
  - `BLANKSIZEX`/`BLANKSIZEY` — заготовка: габарит минус кромки с подрезкой. Пример: 600×1140 с кромкой 1 мм → 598×1138.
- **Текстура.** `TEXTUREANG`: 0 — вдоль X (`TexDir=1`), 1.5707963 — вдоль Y (`TexDir=2`). Если направления нет, элемента нет вовсе.
- **Длина реза.**
  - У `BOARDSMP` `LENGTHCORECUTTING` всегда равен `BLANKSIZEX`.
  - У `BOARDFIG` длина реза берётся вдоль текстуры, а без текстуры — по бо́льшей стороне.
- **Площадь.** `COREMATERIALSQUARE` — площадь заготовки в м², 3 знака после точки.
- **Кромка.**
  - `BOARDSMP`: `EDGEBOT`/`EDGELEFT`/`EDGETOP`/`EDGERIGHT`, `INDEX` = номер сегмента контура.
  - `BOARDFIG`: `EDGES/EDGE` на каждый сегмент.
  - Сторона без кромки — «Гладкий торец» `{5F07E058-…}` без `MATERIAL`.
- **Дуга `CONTITEMARC`.**
  - Задаёт локальную систему с центром в начале координат; в ней дуга всегда идёт против часовой стрелки.
  - При обходе по часовой стрелке матрица `TF` зеркалит оси X и Z (`A00=A22=-1`).
  - `ANGLESTART` лежит в (0, 2π], `ANGLEEND = ANGLESTART + угол дуги`.
  - `X`/`Y` — концы локальных осей.

## Не выгружается (пусто или QTY=0)

- Сверловка `DRILLING`, пазы `SAWCUT`, вырезы `NOTCH`, фрезеровка `MILLING`.
- Фурнитура `BINDINGROOTNODE`.
- Цены и поставщики.
- Контур заготовки у фигурных деталей равен контуру детали.
