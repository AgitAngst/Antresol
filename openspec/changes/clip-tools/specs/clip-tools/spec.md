## ADDED Requirements

### Requirement: Заготовки
The snippets card SHALL copy a saved text to the clipboard on click and SHALL add the current clipboard text as a new snippet.

#### Scenario: Вставить заготовку
- **WHEN** пользователь щёлкает заготовку «Спасибо, получил!»
- **THEN** этот текст в буфере обмена

### Requirement: Пипетка
Dragging the picker circle SHALL show the screen color under the pointer and, on release, MUST put its code into the clipboard.

#### Scenario: Взять цвет
- **WHEN** пользователь тянет круг на зелёную кнопку и отпускает
- **THEN** код цвета в буфере, круг этого цвета, цвет первый в истории

### Requirement: Текст с экрана
The tools card SHALL let the user snip a screen area and MUST put the recognized text into the clipboard.

#### Scenario: Распознать
- **WHEN** пользователь жмёт «Выделить область» и выделяет абзац
- **THEN** распознанный текст в буфере, в карточке — его начало
