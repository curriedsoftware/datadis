# datadis

A Rust library and command-line tool for the [Datadis](https://datadis.es) API,
the Spanish platform that gives access to electricity supply and consumption
data from distributors.

> **Status:** early development. The API surface may change.

## Library

The client logs in on the first request and reuses the bearer token after that.
If the API rejects the token as expired, the client logs in again and retries.

```rust
use datadis::api::{Auth, Client, SuppliesQuery};

#[tokio::main]
async fn main() -> datadis::errors::Result<()> {
    let client = Client::new(Auth {
        username: "12345678A".into(),
        password: "secret".into(),
    });

    let supplies = client.supplies(SuppliesQuery::default()).await?;
    for supply in supplies.supplies {
        println!("{} — {}", supply.cups, supply.address);
    }
    Ok(())
}
```

### Supported endpoints (private API)

| Method                        | Endpoint                             |
| ----------------------------- | ------------------------------------ |
| `Client::supplies`            | `get-supplies-v2`                    |
| `Client::contracts`           | `get-contract-detail-v2`             |
| `Client::consumption`         | `get-consumption-data-v2`            |
| `Client::max_power`           | `get-max-power-v2`                   |
| `Client::distributors`        | `get-distributors-with-supplies-v2`  |
| `Client::reactive_data`       | `get-reactive-data-v2`               |
| `Client::groups`              | `get-groups-v2` (undocumented)       |
| `Client::partners`            | `partner-list`                       |
| `Client::authorizations`      | `list-authorization`                 |
| `Client::new_authorization`   | `new-authorization`                  |
| `Client::cancel_authorization`| `cancel-authorization`               |

Only the v2 endpoints are supported.

### Aggregated searches (public API)

The public API needs no credentials. Pick a search type and `Client::search`
returns that search's result type:

| Search type | Endpoint              | Result             |
| ----------- | --------------------- | ------------------ |
| `Normal`    | `api-search`          | `Vec<NormalData>`  |
| `Sum`       | `api-sum-search`      | `Vec<SumData>`     |
| `Auto`      | `api-search-auto`     | `Vec<AutoData>`    |
| `SumAuto`   | `api-sum-search-auto` | `Vec<SumAutoData>` |

```rust
use datadis::api::{Community, DatasetFilters, Normal};
use datadis::chrono::NaiveDate;

let search = Normal {
    start_date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
    end_date: NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
    community: vec![Community::Madrid],
    page: 0,
    page_size: 100,
    filters: DatasetFilters::default(),
    sort: vec![],
};

// A single page:
let first_page = client.search(search.clone()).await?;

// Or every page, fetched one after another until the last one:
let everything = client.search_all(search).await?;
```

`search_all` works with the paged searches, `Normal` and `Auto`. It starts at
the search's `page` and stops at the first page with fewer than `page_size`
results. A `page_size` of `0` uses the maximum, `MAX_PAGE_SIZE` (2000).

### Dates and times

Dates and timestamps use [`chrono`](https://docs.rs/chrono), re-exported as
`datadis::chrono`:

- Days are `NaiveDate`. Dates that can be absent, like a supply's end date,
  are `Option<NaiveDate>`.
- Consumption and max-power readings have a `datetime: NaiveDateTime` for the
  end of the measured interval, in Spanish local time. Datadis reports the last
  hour of a day as `24:00`, which becomes `00:00` of the next day; `date` keeps
  the day the reading belongs to.
- Queries that Datadis takes by month (consumption, max power, reactive data)
  take a `NaiveDate`, of which only the year and month are sent.

## Command-line tool

```sh
export DATADIS_USERNAME=12345678A
export DATADIS_PASSWORD=secret
cargo run -- supplies
```

You can also pass the credentials with `-u/--username` and `-p/--password`.
Only the `supplies` subcommand works right now.

## License

MIT. See [LICENSE](LICENSE).
