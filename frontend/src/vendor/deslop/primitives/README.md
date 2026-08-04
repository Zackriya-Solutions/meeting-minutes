# Deslop Primitives

Независимая папка с визуальной основой UI kit. Она копируется рядом с
`mini-app` и не требует публикации npm-пакета.

## Состав

- цвета;
- типографика и полный набор шрифтов;
- базовые отступы и радиусы;
- единственная система UI-иконок — Material Symbols Rounded.

Компонентных прокси-токенов здесь нет: компоненты используют базовые значения
вида `--ui-space-16` и `--ui-radius-12` напрямую.

## Документация

- [Цвета](colors.md)
- [Типографика](TYPOGRAPHY.md)

## Иконки

Локальный шрифт содержит полный набор Material Symbols Rounded и не требует
сетевого подключения во время работы приложения. Часто используемые иконки
доступны как именованные экспорты, остальные создаются лениво по официальному
имени символа:

```tsx
import {
  MaterialSymbol,
  getIconComponent,
} from './material-symbols-react';

<MaterialSymbol name="calendar_today" size={20} weight={400} />;

const IconRocketLaunch = getIconComponent('rocket_launch');
<IconRocketLaunch size={20} weight={400} />;
```
