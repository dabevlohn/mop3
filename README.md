# MOP3 - Mastodon/Bluesky to POP3/SMTP Gateway

## Требования

- Rust 1.70+
- Cargo

## Установка

```bash
git clone <repo>
cd mop3
cargo build --release
```

## Архитектура проекта

### Структура модулей

```text
src/
├── main.rs           # Точка входа, инициализация логирования
├── config.rs         # Конфигурация из CLI и env переменных
├── error.rs          # Система обработки ошибок
├── models.rs         # Структуры данных
├── api/
│   ├── mod.rs        # Trait SocialNetworkApi и фабрика
│   ├── mastodon.rs   # Клиент Mastodon API
│   └── bluesky.rs    # Клиент Bluesky API
├── pop3/
│   ├── mod.rs
│   └── server.rs     # Асинхронный POP3 сервер
└── smtp/
    ├── mod.rs
    └── server.rs     # Асинхронный SMTP сервер
```

## Параметры командной строки

```bash
./mop3 [OPTIONS]
```

### Все параметры поддерживают env переменные

| CLI флаг       | Env переменная    | По умолчанию | Описание                                   |
| -------------- | ----------------- | ------------ | ------------------------------------------ |
| `--account`    | `MOP3_ACCOUNT`    | -            | Аккаунт социальной сети (<user@example.com>) |
| `--token`      | `MOP3_TOKEN`      | -            | Токен авторизации API                      |
| `--address`    | `MOP3_ADDRESS`    | `127.0.0.1`  | IP адрес для прослушивания                 |
| `--pop3port`   | `MOP3_POP3_PORT`  | `110`        | POP3 порт                                  |
| `--smtp-port`  | `MOP3_SMTP_PORT`  | `25`         | SMTP порт                                  |
| `--api-mode`   | `MOP3_API_MODE`   | `mastodon`   | API режим: `mastodon` или `bluesky`        |
| `--nosmtp`     | `MOP3_NO_SMTP`    | false        | Отключить SMTP сервер                      |
| `--ascii`      | `MOP3_ASCII`      | false        | Преобразовать Unicode в ASCII              |
| `--attachment` | `MOP3_ATTACHMENT` | false        | Добавлять изображения как вложения         |
| `--inline`     | `MOP3_INLINE`     | false        | Встраивать изображения inline              |
| `--html`       | `MOP3_HTML`       | false        | Отправлять HTML вместо текста              |
| `--debug`      | `MOP3_DEBUG`      | false        | Debug режим                                |
| `--url`        | `MOP3_URL`        | false        | Включать URL оригинального поста           |
| `--proxy`      | `MOP3_PROXY`      | -            | Прокси для ссылок                          |
| `--log-level`  | `RUST_LOG`        | `info`       | Уровень логирования                        |

## Примеры использования

### 1. Запуск с параметрами командной строки (Mastodon)

```bash
./mop3 \
  --account user@mastodon.social \
  --token your_bearer_token \
  --address 0.0.0.0 \
  --pop3port 1110 \
  --smtp-port 1025 \
  --api-mode mastodon
```

### 2. Запуск с env переменными (Mastodon)

```bash
export MOP3_ACCOUNT=user@mastodon.social
export MOP3_TOKEN=your_bearer_token
export MOP3_ADDRESS=0.0.0.0
export MOP3_POP3_PORT=1110
export MOP3_SMTP_PORT=1025
export MOP3_API_MODE=mastodon
export RUST_LOG=debug

./mop3
```

### 3. Запуск с Bluesky API

```bash
export MOP3_ACCOUNT=user.bsky.social
export MOP3_TOKEN=your_bluesky_token
export MOP3_API_MODE=bluesky
export RUST_LOG=info

./mop3
```

### 4. Запуск только POP3 (без SMTP)

```bash
./mop3 \
  --account user@mastodon.social \
  --token token \
  --nosmtp \
  --log-level debug
```

## Многопоточность

Приложение использует асинхронный runtime Tokio:

1. **POP3 сервер** - работает в бесконечном `loop` через `tokio::spawn`
2. **SMTP сервер** - работает в отдельной задаче через `tokio::spawn`
3. **Каждое соединение** - обрабатывается в отдельной async задаче
4. **HTTP запросы** - не блокируют, имеют timeout 30 секунд

### Преимущества

- ✅ Нет паники при timeout API
- ✅ Сервер всегда доступен
- ✅ Высокая пропускная способность
- ✅ Минимальное использование памяти

## API режимы

### Mastodon API

- Полная поддержка получения ленты
- Отправка постов
- Загрузка медиа (изображения, видео)
- Поддержка ответов на посты

```bash
export MOP3_API_MODE=mastodon
export MOP3_ACCOUNT=user@mastodon.social
export MOP3_TOKEN=your_mastodon_token

./mop3
```

### Bluesky API

- Базовая аутентификация
- Скелет для расширения функциональности

```bash
export MOP3_API_MODE=bluesky
export MOP3_ACCOUNT=user.bsky.social
export MOP3_TOKEN=your_bluesky_token

./mop3
```

## Планы развития

- [ ] Полная реализация Bluesky API
- [ ] Кэширование ленты
- [ ] WebSocket поддержка
- [ ] Metrics и мониторинг
- [ ] OAuth2 для веб-клиентов
- [ ] Поддержка других социальных сетей

## Лицензия

MIT

## Контакты

Проект инициирован Nathan Kiesman [https://github.com/miakizz/mop3](https://github.com/miakizz/mop3)
