# RustWeb — projekt v0.1

**Data:** 2026-11-01
**Status:** zaakceptowany przez delegację („rób jak chcesz")

## Cel

RustWeb to framework webowy dla Rusta w duchu FastAPI: deklarujesz typy wejścia
i wyjścia, a framework daje walidację, spójne błędy 422, automatyczny OpenAPI
i Swagger UI. Priorytetem jest niskie opóźnienie — warstwa frameworka nie może
dokładać pracy w hot path.

## Dlaczego

FastAPI zawdzięcza ergonomię runtime introspection Pythona. Rust jej nie ma,
więc odpowiednikiem są makra proceduralne: `#[rustweb::get]`, `#[rustweb::model]`,
`ApiJson<T>`. Makra rozwijają się w czasie kompilacji, więc nie kosztują nic
w czasie żądania. Cały narzut runtime to walidacja danych wejściowych, której
i tak nie da się uniknąć.

## Zakres v0.1

- Routing: `#[get]`, `#[post]`, `#[put]`, `#[delete]`, `#[patch]` z path params.
- Ekstraktory: `ApiPath<T>`, `ApiQuery<T>`, `ApiJson<T>` (walidacja + 422).
- Modele: `#[model]` dodający `Serialize`, `Deserialize`, `ToSchema`, `Validate`.
- Błędy: `ApiError` / `ApiResult<T>` z ciałem `{"detail": ...}` jak w FastAPI.
- OpenAPI 3.2: automatyczne ścieżki, parametry, request body, response, schematy.
- Swagger UI pod `/docs`, spec pod `/openapi.json`.
- Fallback 404 w formacie JSON.
- Benchmark p50/p99 na tle czystego axum.

## Poza zakresem v0.1

- Własny serwer HTTP, TLS, HTTP/3, graceful shutdown.
- Pełna zgodność semantyczna z Pydantic (koercja typów, `response_model`).
- Per-request dependency injection (`Depends`) i stan aplikacji.
- CLI, migracje, ORM, „batteries included".
- Auto-rejestracja tras przez link-time inventory.
- WebSockety i streaming.

## Decyzje architektoniczne

1. **Baza:** axum 0.8 + tower + hyper + tokio. Nie piszemy HTTP od zera;
   axum jest dojrzały i szybki, a my dokładamy wyłącznie warstwę DX.
2. **Handler:** makro zamienia funkcję w unit struct (wzorzec znany z Salvo).
   Rejestracja to `.route(get_user)`, bez listy `routes![]`.
3. **OpenAPI:** budowane raz przy starcie z metadanych tras; zero pracy
   per request. Schematy przez `utoipa::ToSchema`, bez ręcznego
   `#[utoipa::path]`.
4. **Walidacja:** `validator` + `ApiJson`/`ApiQuery`; błędy mapowane na 422
   z `detail[]{loc,msg,type,ctx}`.
5. **Wydajność:** brak dodatkowych layerów w hot path; benchmark od pierwszego
   dnia, porównanie z czystym axum.

## API v0.1

```rust
use rustweb::prelude::*;

#[rustweb::model]
struct CreateUser {
    #[validate(length(min = 3, max = 50))]
    name: String,
    #[validate(range(min = 0, max = 150))]
    age: u8,
}

#[rustweb::model]
struct User {
    id: u64,
    name: String,
    age: u8,
}

#[rustweb::get("/users/{id}")]
async fn get_user(ApiPath(id): ApiPath<u64>) -> ApiResult<Json<User>> {
    Ok(Json(User { id, name: "Ada".into(), age: 36 }))
}

#[rustweb::post("/users", status = 201, tags = ["users"])]
async fn create_user(ApiJson(body): ApiJson<CreateUser>) -> ApiResult<Json<User>> {
    Ok(Json(User { id: 1, name: body.name, age: body.age }))
}

#[tokio::main]
async fn main() {
    RustWeb::new("RustWeb Demo", "0.1.0")
        .route(get_user)
        .route(create_user)
        .serve("127.0.0.1:3000")
        .await
        .unwrap();
}
```

## Ograniczenia i znane różnice

- Handler nie jest bezpośrednio wywoływalny — to unit struct. Logikę biznesową
  należy wydzielić do zwykłych funkcji.
- Brak koercji typów: `"36"` nie przejdzie jako `u8`. To świadoma różnica
  względem Pydantic lax mode.
- `#[model]` wymaga, by crate użytkownika miało `serde`, `utoipa` i `validator`
  jako bezpośrednie zależności (makro generuje `#[derive(...)]` z tymi ścieżkami).
- Stan aplikacji i DI poza v0.1; `Router<()>`.
- Nazwy schematów OpenAPI są globalne; dwa typy o tej samej nazwie w różnych
  modułach kolidują (sposób rozwiązania: `#[schema(as = ...)]`).

## Ryzyka

- Ręczne generowanie operacji OpenAPI może się rozjechać z utoipą — testy
  integracyjne sprawdzają kształt dokumentu.
- `#[model]` dodaje derive'y; konflikt z ręcznymi derive'ami — makro wykrywa
  i pomija istniejące.
- axum jest przed 1.0 — wersja przypięta do 0.8.
- Swagger UI vendored zwiększa binarkę — feature można wyłączyć w przyszłości.
- Benchmarki są środowiskowe; wyniki mają character porównawczy, nie absolutny.

## Kryteria sukcesu

- `cargo test --workspace` przechodzi.
- Przykład działa: `GET /users/1`, `POST /users`, `/docs`, `/openapi.json`.
- Niepoprawne body daje 422 z `detail[0].loc == ["body", "name"]`.
- OpenAPI zawiera ścieżki, parametry i schematy bez ręcznej konfiguracji.
- Hello-world na RustWeb jest w granicach szumu wobec czystego axum.
