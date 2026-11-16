/***
 * Copyright (c) 2026 Rafael Fernández López <ereslibre@curried.software>
 *
 * Permission is hereby granted, free of charge, to any person
 * obtaining a copy of this software and associated documentation
 * files (the "Software"), to deal in the Software without
 * restriction, including without limitation the rights to use, copy,
 * modify, merge, publish, distribute, sublicense, and/or sell copies
 * of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be
 * included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
 * NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS
 * BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN
 * ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
 * CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 ***/

use crate::errors::{Error, Result};
use chrono::{NaiveDate, NaiveDateTime};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::time::Duration;
use tokio::sync::Mutex;

/// Datadis authentication endpoint. Returns the bearer token, as plain text,
/// that authenticates every subsequent request.
const LOGIN_URL: &str = "https://datadis.es/nikola-auth/tokens/login";

/// Base URL for every authenticated API endpoint.
const BASE_URL: &str = "https://datadis.es/api-private/api";

/// Base URL for the public, unauthenticated aggregated-search API.
const PUBLIC_BASE_URL: &str = "https://datadis.es/api-public";

/// Datadis queries every distributor in real time and is often slow.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

pub struct Auth {
    pub username: String,
    pub password: String,
}

pub struct Client {
    auth: Auth,
    http: reqwest::Client,
    /// Bearer token, lazily fetched on the first request and reused until the
    /// API rejects it.
    token: Mutex<Option<String>>,
}

/// Electricity distributor, identified on the wire by its numeric code.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub enum DistributorCode {
    Viesgo,
    EDistribucion,
    ERedes,
    Aseme,
    Ufd,
    Eosa,
    Cide,
    Ide,
    /// Any code not covered by the documented enumeration.
    Other(String),
}

impl From<String> for DistributorCode {
    fn from(code: String) -> Self {
        match code.as_str() {
            "1" => DistributorCode::Viesgo,
            "2" => DistributorCode::EDistribucion,
            "3" => DistributorCode::ERedes,
            "4" => DistributorCode::Aseme,
            "5" => DistributorCode::Ufd,
            "6" => DistributorCode::Eosa,
            "7" => DistributorCode::Cide,
            "8" => DistributorCode::Ide,
            _ => DistributorCode::Other(code),
        }
    }
}

impl std::fmt::Display for DistributorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DistributorCode::Viesgo => f.write_str("Viesgo"),
            DistributorCode::EDistribucion => f.write_str("E-distribución"),
            DistributorCode::ERedes => f.write_str("E-redes"),
            DistributorCode::Aseme => f.write_str("ASEME"),
            DistributorCode::Ufd => f.write_str("UFD"),
            DistributorCode::Eosa => f.write_str("EOSA"),
            DistributorCode::Cide => f.write_str("CIDE"),
            DistributorCode::Ide => f.write_str("I-DE"),
            DistributorCode::Other(code) => f.write_str(code),
        }
    }
}

/// Per-distributor error returned alongside the data in every v2 response.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributorError {
    pub distributor_code: DistributorCode,
    pub distributor_name: String,
    pub error_code: String,
    pub error_description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Supply {
    pub address: String,
    pub cups: String,
    pub postal_code: String,
    pub province: String,
    pub municipality: String,
    pub distributor: String,
    #[serde(deserialize_with = "dates::date")]
    pub valid_date_from: NaiveDate,
    /// `None` while the supply has no end date.
    #[serde(default, deserialize_with = "dates::optional_date")]
    pub valid_date_to: Option<NaiveDate>,
    pub point_type: u8,
    pub distributor_code: DistributorCode,
}

/// Response of `/get-supplies-v2`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Supplies {
    pub supplies: Vec<Supply>,
    #[serde(default)]
    pub distributor_error: Vec<DistributorError>,
}

/// A span during which the querying user was the owner of the CUPS.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateOwner {
    #[serde(deserialize_with = "dates::date")]
    pub start_date: NaiveDate,
    /// `None` while the user is still the owner.
    #[serde(default, deserialize_with = "dates::optional_date")]
    pub end_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub cups: String,
    pub distributor: String,
    /// Only returned when the querying user owns the CUPS.
    #[serde(default)]
    pub marketer: Option<String>,
    pub tension: String,
    pub access_fare: String,
    pub province: String,
    pub municipality: String,
    pub postal_code: String,
    #[serde(rename = "contractedPowerkW")]
    pub contracted_power_kw: Vec<f64>,
    pub time_discrimination: String,
    pub mode_power_control: String,
    #[serde(deserialize_with = "dates::date")]
    pub start_date: NaiveDate,
    /// `None` while the contract has no end date.
    #[serde(default, deserialize_with = "dates::optional_date")]
    pub end_date: Option<NaiveDate>,
    pub code_fare: String,
    // The fields below are only informed when the supply has self-consumption.
    #[serde(default)]
    pub self_consumption_type_code: Option<String>,
    #[serde(default)]
    pub self_consumption_type_desc: Option<String>,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub subsection: Option<String>,
    #[serde(default)]
    pub partition_coefficient: Option<f64>,
    #[serde(default)]
    pub cau: Option<String>,
    #[serde(default, rename = "installedCapacityKW")]
    pub installed_capacity_kw: Option<f64>,
    #[serde(default)]
    pub date_owner: Vec<DateOwner>,
    #[serde(default, deserialize_with = "dates::optional_date")]
    pub last_marketer_date: Option<NaiveDate>,
    #[serde(default)]
    pub max_power_install: Option<String>,
}

/// Response of `/get-contract-detail-v2`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contracts {
    pub contract: Vec<Contract>,
    #[serde(default)]
    pub distributor_error: Vec<DistributorError>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "RawTimeCurve")]
pub struct TimeCurve {
    pub cups: String,
    /// Day the measurement belongs to.
    pub date: NaiveDate,
    /// End of the measured interval, in Spanish local time. Datadis reports
    /// the last interval of a day as `24:00`, which becomes `00:00` of the
    /// following day here; [`TimeCurve::date`] keeps the day it belongs to.
    pub datetime: NaiveDateTime,
    pub consumption_k_wh: f64,
    pub obtain_method: String,
    pub surplus_energy_k_wh: Option<f64>,
    pub generation_energy_k_wh: Option<f64>,
    pub self_consumption_energy_k_wh: Option<f64>,
}

/// Wire shape of [`TimeCurve`], with separate `date` and `time` fields.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTimeCurve {
    cups: String,
    #[serde(deserialize_with = "dates::date")]
    date: NaiveDate,
    time: String,
    consumption_k_wh: f64,
    obtain_method: String,
    #[serde(default)]
    surplus_energy_k_wh: Option<f64>,
    #[serde(default)]
    generation_energy_k_wh: Option<f64>,
    #[serde(default)]
    self_consumption_energy_k_wh: Option<f64>,
}

impl TryFrom<RawTimeCurve> for TimeCurve {
    type Error = String;

    fn try_from(raw: RawTimeCurve) -> std::result::Result<Self, Self::Error> {
        Ok(TimeCurve {
            datetime: dates::measurement(raw.date, &raw.time)?,
            cups: raw.cups,
            date: raw.date,
            consumption_k_wh: raw.consumption_k_wh,
            obtain_method: raw.obtain_method,
            surplus_energy_k_wh: raw.surplus_energy_k_wh,
            generation_energy_k_wh: raw.generation_energy_k_wh,
            self_consumption_energy_k_wh: raw.self_consumption_energy_k_wh,
        })
    }
}

/// Response of `/get-consumption-data-v2`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Consumption {
    pub time_curve: Vec<TimeCurve>,
    #[serde(default)]
    pub distributor_error: Vec<DistributorError>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "RawMaxPower")]
pub struct MaxPower {
    pub cups: String,
    /// Day the maximum was demanded.
    pub date: NaiveDate,
    /// When the maximum was demanded, in Spanish local time. `24:00` becomes
    /// `00:00` of the following day, as in [`TimeCurve::datetime`].
    pub datetime: NaiveDateTime,
    /// Watts.
    pub max_power: f64,
    pub period: String,
}

/// Wire shape of [`MaxPower`], with separate `date` and `time` fields.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMaxPower {
    cups: String,
    #[serde(deserialize_with = "dates::date")]
    date: NaiveDate,
    time: String,
    max_power: f64,
    period: String,
}

impl TryFrom<RawMaxPower> for MaxPower {
    type Error = String;

    fn try_from(raw: RawMaxPower) -> std::result::Result<Self, Self::Error> {
        Ok(MaxPower {
            datetime: dates::measurement(raw.date, &raw.time)?,
            cups: raw.cups,
            date: raw.date,
            max_power: raw.max_power,
            period: raw.period,
        })
    }
}

/// Response of `/get-max-power-v2`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaxPowers {
    pub max_power: Vec<MaxPower>,
    #[serde(default)]
    pub distributor_error: Vec<DistributorError>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributorsWithSupplies {
    pub distributor_codes: Vec<DistributorCode>,
}

/// Response of `/get-distributors-with-supplies-v2`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Distributors {
    pub dist_existence_user: DistributorsWithSupplies,
    #[serde(default)]
    pub distributor_error: Vec<DistributorError>,
}

/// Reactive energy for a single month, split across the six tariff periods.
#[derive(Debug, Clone, Deserialize)]
pub struct ReactiveEnergyPeriod {
    /// First day of the month the values belong to (sent as `AAAA/MM`).
    #[serde(deserialize_with = "dates::month")]
    pub date: NaiveDate,
    pub energy_p1: f64,
    pub energy_p2: f64,
    pub energy_p3: f64,
    pub energy_p4: f64,
    pub energy_p5: f64,
    pub energy_p6: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReactiveEnergy {
    pub cups: String,
    pub energy: Vec<ReactiveEnergyPeriod>,
    pub code: String,
    pub code_desc: String,
}

/// Response of `/get-reactive-data-v2`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactiveData {
    pub reactive_energy: ReactiveEnergy,
    #[serde(default)]
    pub distributor_error: Vec<DistributorError>,
}

/// An item of `/get-groups-v2`.
#[derive(Debug, Clone, Deserialize)]
pub struct Group {
    pub name: String,
    pub description: String,
}

/// An item of `/list-authorization`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Authorization {
    pub id: i64,
    pub owner_document: String,
    pub requester_document: String,
    /// Not part of the documented response; kept optional in case it is sent.
    #[serde(default)]
    pub cups: Option<String>,
    pub status: String,
    #[serde(default, deserialize_with = "dates::optional_date")]
    pub validity_date_start: Option<NaiveDate>,
    #[serde(default, deserialize_with = "dates::optional_date")]
    pub validity_date_end: Option<NaiveDate>,
    pub distributor_code_father: String,
}

/// An item of `/partner-list`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Partner {
    pub name: String,
    pub document: String,
    pub email: String,
    /// Sent as e.g. `2026-03-11 00:00:00.0`.
    #[serde(deserialize_with = "dates::datetime")]
    pub registration_date: NaiveDateTime,
}

/// Response of `/new-authorization` and `/cancel-authorization`.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthorizationResult {
    pub error: String,
    pub message: String,
}

/// Query parameters for [`Client::supplies`].
#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppliesQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorized_nif: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distributor_code: Option<String>,
}

/// Query parameters for [`Client::contracts`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractsQuery {
    pub cups: String,
    pub distributor_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorized_nif: Option<String>,
}

/// Query parameters for [`Client::consumption`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumptionQuery {
    pub cups: String,
    pub distributor_code: String,
    /// First month to query. Only the year and month are sent.
    #[serde(serialize_with = "dates::serialize_month")]
    pub start_date: NaiveDate,
    /// Last month to query. Only the year and month are sent.
    #[serde(serialize_with = "dates::serialize_month")]
    pub end_date: NaiveDate,
    /// `0` for hourly consumption, `1` for quarter-hourly.
    pub measurement_type: String,
    pub point_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorized_nif: Option<String>,
}

/// Query parameters for [`Client::max_power`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaxPowerQuery {
    pub cups: String,
    pub distributor_code: String,
    /// First month to query. Only the year and month are sent.
    #[serde(serialize_with = "dates::serialize_month")]
    pub start_date: NaiveDate,
    /// Last month to query. Only the year and month are sent.
    #[serde(serialize_with = "dates::serialize_month")]
    pub end_date: NaiveDate,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorized_nif: Option<String>,
}

/// Query parameters for [`Client::distributors`].
#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributorsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorized_nif: Option<String>,
}

/// Query parameters for [`Client::reactive_data`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactiveDataQuery {
    pub cups: String,
    pub distributor_code: String,
    /// First month to query. Only the year and month are sent.
    #[serde(serialize_with = "dates::serialize_month")]
    pub start_date: NaiveDate,
    /// Last month to query. Only the year and month are sent.
    #[serde(serialize_with = "dates::serialize_month")]
    pub end_date: NaiveDate,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorized_nif: Option<String>,
}

/// Query parameters for [`Client::authorizations`].
#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_nif: Option<String>,
}

/// Query parameters for [`Client::new_authorization`].
///
/// `cups` is serialized as repeated `cups=` query keys; an empty list
/// authorizes every CUPS of the user.
#[derive(Debug, Clone)]
pub struct NewAuthorizationQuery {
    pub authorized_nif: String,
    /// Sent as `AAAA/MM/dd`.
    pub start_date: Option<NaiveDate>,
    /// Sent as `AAAA/MM/dd`.
    pub end_date: Option<NaiveDate>,
    pub cups: Vec<String>,
}

/// Query parameters for [`Client::cancel_authorization`].
///
/// `cups` is serialized as repeated `cups=` query keys; an empty list cancels
/// every CUPS of the user.
#[derive(Debug, Clone)]
pub struct CancelAuthorizationQuery {
    pub authorized_nif: String,
    pub cups: Vec<String>,
}

/// (De)serializers for the date and time formats Datadis uses.
mod dates {
    use chrono::{Days, NaiveDate, NaiveDateTime, NaiveTime};
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Datadis documents `AAAA/MM/dd`; dashes are accepted too.
    const DATE_FORMATS: &[&str] = &["%Y/%m/%d", "%Y-%m-%d"];

    const DATETIME_FORMATS: &[&str] = &[
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y/%m/%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
        "%Y/%m/%d %H:%M",
    ];

    pub fn parse_date(value: &str) -> Result<NaiveDate, String> {
        let value = value.trim();
        DATE_FORMATS
            .iter()
            .find_map(|format| NaiveDate::parse_from_str(value, format).ok())
            .or_else(|| parse_datetime(value).ok().map(|datetime| datetime.date()))
            .ok_or_else(|| format!("invalid date `{value}`"))
    }

    pub fn parse_datetime(value: &str) -> Result<NaiveDateTime, String> {
        let value = value.trim();
        DATETIME_FORMATS
            .iter()
            .find_map(|format| NaiveDateTime::parse_from_str(value, format).ok())
            .ok_or_else(|| format!("invalid date and time `{value}`"))
    }

    /// Parses `AAAA/MM` into the first day of that month.
    pub fn parse_month(value: &str) -> Result<NaiveDate, String> {
        let value = value.trim();
        parse_date(&format!("{}/01", value.replace('-', "/")))
            .map_err(|_| format!("invalid month `{value}`"))
    }

    /// Combines a measurement day with its `hh:mm` time. `24:00` is
    /// midnight at the end of `date`, so it becomes `00:00` of the next day.
    pub fn measurement(date: NaiveDate, time: &str) -> Result<NaiveDateTime, String> {
        let time = time.trim();
        if time == "24:00" {
            return date
                .checked_add_days(Days::new(1))
                .map(|next| next.and_time(NaiveTime::MIN))
                .ok_or_else(|| format!("date out of range `{date}`"));
        }
        NaiveTime::parse_from_str(time, "%H:%M")
            .map(|time| date.and_time(time))
            .map_err(|_| format!("invalid time `{time}`"))
    }

    pub fn date<'de, D: Deserializer<'de>>(deserializer: D) -> Result<NaiveDate, D::Error> {
        parse_date(&String::deserialize(deserializer)?).map_err(D::Error::custom)
    }

    /// Missing, null and empty values become `None`.
    pub fn optional_date<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<NaiveDate>, D::Error> {
        match Option::<String>::deserialize(deserializer)? {
            Some(value) if !value.trim().is_empty() => {
                parse_date(&value).map(Some).map_err(D::Error::custom)
            }
            _ => Ok(None),
        }
    }

    pub fn month<'de, D: Deserializer<'de>>(deserializer: D) -> Result<NaiveDate, D::Error> {
        parse_month(&String::deserialize(deserializer)?).map_err(D::Error::custom)
    }

    pub fn datetime<'de, D: Deserializer<'de>>(deserializer: D) -> Result<NaiveDateTime, D::Error> {
        parse_datetime(&String::deserialize(deserializer)?).map_err(D::Error::custom)
    }

    /// Serializes as `AAAA/MM`.
    pub fn serialize_month<S: Serializer>(
        date: &NaiveDate,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&date.format("%Y/%m"))
    }

    /// Formats as `AAAA/MM/dd`.
    pub fn format_day(date: NaiveDate) -> String {
        date.format("%Y/%m/%d").to_string()
    }
}

/// Lenient deserializers for the public search API, whose documented samples
/// send numeric fields either as JSON numbers or as numeric strings.
mod lenient {
    use serde::Deserialize;
    use serde::de::{Deserializer, Error};
    use serde_json::{Map, Value};
    use std::str::FromStr;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumberOrString<T> {
        Number(T),
        String(String),
    }

    /// A number encoded either as a JSON number or as a numeric string.
    pub fn number<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de> + FromStr,
    {
        match NumberOrString::<T>::deserialize(deserializer)? {
            NumberOrString::Number(number) => Ok(number),
            NumberOrString::String(string) => string
                .trim()
                .parse()
                .map_err(|_| D::Error::custom(format!("invalid number `{string}`"))),
        }
    }

    /// Collects the hourly `mi1`..`mi25` fields, in order, into a vector.
    /// Missing, null or empty values become `None`.
    pub fn hourly<'de, D>(deserializer: D) -> Result<Vec<Option<f64>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = Map::<String, Value>::deserialize(deserializer)?;
        (1..=25)
            .map(|hour| match fields.get(&format!("mi{hour}")) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::Number(number)) => Ok(number.as_f64()),
                Some(Value::String(string)) if string.trim().is_empty() => Ok(None),
                Some(Value::String(string)) => string
                    .trim()
                    .parse()
                    .map(Some)
                    .map_err(|_| D::Error::custom(format!("invalid mi{hour} value `{string}`"))),
                Some(other) => Err(D::Error::custom(format!(
                    "unexpected mi{hour} value `{other}`"
                ))),
            })
            .collect()
    }
}

/// Query string builder for the public search API. Multi-valued parameters
/// are sent comma-separated, as the documentation describes.
#[derive(Default)]
struct Params(Vec<(&'static str, String)>);

impl Params {
    fn set(&mut self, key: &'static str, value: impl ToString) {
        self.0.push((key, value.to_string()));
    }

    fn list(&mut self, key: &'static str, values: &[String]) {
        if !values.is_empty() {
            self.0.push((key, values.join(",")));
        }
    }

    fn dates(start_date: NaiveDate, end_date: NaiveDate, community: &[Community]) -> Params {
        let mut params = Params::default();
        params.set("startDate", dates::format_day(start_date));
        params.set("endDate", dates::format_day(end_date));
        if !community.is_empty() {
            let codes: Vec<&str> = community.iter().map(|c| c.code()).collect();
            params.set("community", codes.join(","));
        }
        params
    }
}

/// Spanish autonomous community, as accepted by the `community` parameter of
/// the public search API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Community {
    Andalucia,
    Aragon,
    Asturias,
    IslasBaleares,
    Canarias,
    Cantabria,
    CastillaYLeon,
    CastillaLaMancha,
    Cataluna,
    ComunidadValenciana,
    Extremadura,
    Galicia,
    Madrid,
    Murcia,
    Navarra,
    PaisVasco,
    LaRioja,
    Ceuta,
    Melilla,
}

impl Community {
    /// Every community, in code order.
    pub const ALL: [Community; 19] = [
        Community::Andalucia,
        Community::Aragon,
        Community::Asturias,
        Community::IslasBaleares,
        Community::Canarias,
        Community::Cantabria,
        Community::CastillaYLeon,
        Community::CastillaLaMancha,
        Community::Cataluna,
        Community::ComunidadValenciana,
        Community::Extremadura,
        Community::Galicia,
        Community::Madrid,
        Community::Murcia,
        Community::Navarra,
        Community::PaisVasco,
        Community::LaRioja,
        Community::Ceuta,
        Community::Melilla,
    ];

    /// Two-digit code sent on the wire, e.g. `13` for Comunidad de Madrid.
    pub const fn code(self) -> &'static str {
        match self {
            Community::Andalucia => "01",
            Community::Aragon => "02",
            Community::Asturias => "03",
            Community::IslasBaleares => "04",
            Community::Canarias => "05",
            Community::Cantabria => "06",
            Community::CastillaYLeon => "07",
            Community::CastillaLaMancha => "08",
            Community::Cataluna => "09",
            Community::ComunidadValenciana => "10",
            Community::Extremadura => "11",
            Community::Galicia => "12",
            Community::Madrid => "13",
            Community::Murcia => "14",
            Community::Navarra => "15",
            Community::PaisVasco => "16",
            Community::LaRioja => "17",
            Community::Ceuta => "18",
            Community::Melilla => "19",
        }
    }

    /// Name as given in the Datadis documentation.
    pub const fn name(self) -> &'static str {
        match self {
            Community::Andalucia => "Andalucía",
            Community::Aragon => "Aragón",
            Community::Asturias => "Principado de Asturias",
            Community::IslasBaleares => "Islas Baleares",
            Community::Canarias => "Canarias",
            Community::Cantabria => "Cantabria",
            Community::CastillaYLeon => "Castilla y León",
            Community::CastillaLaMancha => "Castilla La Mancha",
            Community::Cataluna => "Cataluña",
            Community::ComunidadValenciana => "Comunidad Valenciana",
            Community::Extremadura => "Extremadura",
            Community::Galicia => "Galicia",
            Community::Madrid => "Comunidad de Madrid",
            Community::Murcia => "Región de Murcia",
            Community::Navarra => "Comunidad Foral de Navarra",
            Community::PaisVasco => "País Vasco",
            Community::LaRioja => "La Rioja",
            Community::Ceuta => "Ceuta",
            Community::Melilla => "Melilla",
        }
    }

    /// Looks a community up by its two-digit code.
    pub fn from_code(code: &str) -> Option<Community> {
        Community::ALL.into_iter().find(|c| c.code() == code)
    }
}

impl std::fmt::Display for Community {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Optional filters shared by the [`Normal`] and [`Sum`] searches.
///
/// Every field accepts several codes; see the Datadis public API
/// documentation for the valid code of each one.
#[derive(Debug, Default, Clone)]
pub struct DatasetFilters {
    /// Measure point type, `01` to `05`.
    pub measurement_type: Vec<String>,
    /// Four-digit distributor code, e.g. `0172`.
    pub distributor: Vec<String>,
    /// Fare code, e.g. `2T`, `30`, `6A`.
    pub fare: Vec<String>,
    /// Two-digit province code or five-digit province + municipality code,
    /// e.g. `03` or `03133`.
    pub province_municipality: Vec<String>,
    /// Group results by postal code. Requires at least one municipality in
    /// [`DatasetFilters::province_municipality`].
    pub group_by_postal_code: Option<bool>,
    pub postal_code: Vec<String>,
    /// `1` residential, `2` industrial, `3` services, `4` not specified.
    pub economic_sector: Vec<String>,
    /// Tension code, `E0` to `E6`.
    pub tension: Vec<String>,
    /// Time discrimination code, e.g. `G0`, `E1`, `E2`, `E3`.
    pub time_discrimination: Vec<String>,
}

impl DatasetFilters {
    fn append_to(&self, params: &mut Params) {
        params.list("measurementType", &self.measurement_type);
        params.list("distributor", &self.distributor);
        params.list("fare", &self.fare);
        params.list("provinceMunicipality", &self.province_municipality);
        if let Some(group) = self.group_by_postal_code {
            params.set("groupByPostalCode", u8::from(group));
        }
        params.list("postalCode", &self.postal_code);
        params.list("economicSector", &self.economic_sector);
        params.list("tension", &self.tension);
        params.list("timeDiscrimination", &self.time_discrimination);
    }
}

/// Optional filters shared by the [`Auto`] and [`SumAuto`] self-consumption
/// searches.
#[derive(Debug, Default, Clone)]
pub struct SelfConsumptionFilters {
    /// Four-digit distributor code, e.g. `0172`.
    pub distributor: Vec<String>,
    /// Self-consumption type code, e.g. `31`, `41`, `51`.
    pub self_consumption: Vec<String>,
    /// Two-digit province code, e.g. `03`.
    pub province: Vec<String>,
}

impl SelfConsumptionFilters {
    fn append_to(&self, params: &mut Params) {
        params.list("distributor", &self.distributor);
        params.list("selfConsumption", &self.self_consumption);
        params.list("province", &self.province);
    }
}

/// `/api-search`: paginated consumption data aggregated by dataset.
#[derive(Debug, Clone)]
pub struct Normal {
    /// Sent as `YYYY/MM/dd`.
    pub start_date: NaiveDate,
    /// Sent as `YYYY/MM/dd`.
    pub end_date: NaiveDate,
    /// At least one community, at most two.
    pub community: Vec<Community>,
    /// Zero-based page number.
    pub page: u32,
    /// Results per page, at most 2000.
    pub page_size: u32,
    pub filters: DatasetFilters,
    /// Fields to sort by; prefix a field with `-` for reverse order. Only
    /// fields used as filters can be sorted on.
    pub sort: Vec<String>,
}

/// `/api-sum-search`: totals of consumption data aggregated by dataset.
#[derive(Debug, Clone)]
pub struct Sum {
    /// Sent as `YYYY/MM/dd`.
    pub start_date: NaiveDate,
    /// Sent as `YYYY/MM/dd`.
    pub end_date: NaiveDate,
    /// At least one community, at most two.
    pub community: Vec<Community>,
    pub filters: DatasetFilters,
    /// Fields to sort by; prefix a field with `-` for reverse order.
    pub sort: Vec<String>,
}

/// `/api-search-auto`: paginated self-consumption data.
#[derive(Debug, Clone)]
pub struct Auto {
    /// Sent as `YYYY/MM/dd`.
    pub start_date: NaiveDate,
    /// Sent as `YYYY/MM/dd`.
    pub end_date: NaiveDate,
    /// At least one community, at most two.
    pub community: Vec<Community>,
    /// Zero-based page number.
    pub page: u32,
    /// Results per page, at most 2000.
    pub page_size: u32,
    pub filters: SelfConsumptionFilters,
    /// Fields to sort by; prefix a field with `-` for reverse order.
    pub sort: Vec<String>,
}

/// `/api-sum-search-auto`: totals of self-consumption data.
#[derive(Debug, Clone)]
pub struct SumAuto {
    /// Sent as `YYYY/MM/dd`.
    pub start_date: NaiveDate,
    /// Sent as `YYYY/MM/dd`.
    pub end_date: NaiveDate,
    /// At least one community, at most two.
    pub community: Vec<Community>,
    pub filters: SelfConsumptionFilters,
}

/// An item of `/api-search`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalData {
    #[serde(deserialize_with = "lenient::number")]
    pub data_day: u32,
    #[serde(deserialize_with = "lenient::number")]
    pub data_month: u32,
    #[serde(deserialize_with = "lenient::number")]
    pub data_year: i32,
    pub community: String,
    pub province: String,
    pub municipality: String,
    pub postal_code: String,
    pub fare: String,
    pub time_discrimination: String,
    pub measure_point_type: String,
    #[serde(deserialize_with = "lenient::number")]
    pub sum_contracts: u64,
    /// kWh.
    #[serde(deserialize_with = "lenient::number")]
    pub sum_energy: f64,
    pub tension: String,
    pub economic_sector: String,
    pub distributor: String,
    /// Energy consumed per hour (`mi1`..`mi25`), in kWh. Index 0 is the first
    /// hour; the 25th slot only applies to the day daylight saving time ends.
    #[serde(flatten, deserialize_with = "lenient::hourly")]
    pub hourly: Vec<Option<f64>>,
}

impl NormalData {
    /// The day this row aggregates, if `dataDay`, `dataMonth` and `dataYear`
    /// form a valid date.
    pub fn date(&self) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(self.data_year, self.data_month, self.data_day)
    }
}

/// An item of `/api-sum-search`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SumData {
    /// kWh.
    #[serde(deserialize_with = "lenient::number")]
    pub sum_energy: f64,
    #[serde(deserialize_with = "lenient::number")]
    pub sum_contract: u64,
}

/// An item of `/api-search-auto`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoData {
    #[serde(deserialize_with = "lenient::number")]
    pub data_day: u32,
    #[serde(deserialize_with = "lenient::number")]
    pub data_month: u32,
    #[serde(deserialize_with = "lenient::number")]
    pub data_year: i32,
    pub community: String,
    pub province: String,
    pub self_consumption: String,
    #[serde(deserialize_with = "lenient::number")]
    pub sum_contracts: u64,
    /// Energy fed into the grid, in kWh.
    #[serde(deserialize_with = "lenient::number")]
    pub sum_energy: f64,
    /// Generation power, in kW.
    #[serde(deserialize_with = "lenient::number")]
    pub sum_power: f64,
    pub distributor: String,
    /// Energy fed into the grid per hour (`mi1`..`mi25`), in kWh.
    #[serde(flatten, deserialize_with = "lenient::hourly")]
    pub hourly: Vec<Option<f64>>,
}

impl AutoData {
    /// The day this row aggregates, if `dataDay`, `dataMonth` and `dataYear`
    /// form a valid date.
    pub fn date(&self) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(self.data_year, self.data_month, self.data_day)
    }
}

/// An item of `/api-sum-search-auto`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SumAutoData {
    /// Energy fed into the grid, in kWh.
    #[serde(deserialize_with = "lenient::number")]
    pub sum_energy: f64,
    /// Generation power, in kW.
    #[serde(deserialize_with = "lenient::number")]
    pub sum_power: f64,
    #[serde(deserialize_with = "lenient::number")]
    pub sum_contract: u64,
}

/// A kind of aggregated-data search on the public, unauthenticated API.
///
/// Each search type resolves, at compile time, to the result shape that the
/// search produces via its [`SearchType::Output`] associated type. Callers pick
/// a concrete type and get that result back directly — no matching required.
///
/// Sealed: only the search types defined in this crate may implement it.
pub trait SearchType: sealed::Sealed {
    /// The result shape this search produces.
    type Output;

    /// Runs the search against `client`, producing [`Self::Output`].
    fn run(&self, client: &Client) -> impl Future<Output = Result<Self::Output>>;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::Normal {}
    impl Sealed for super::Sum {}
    impl Sealed for super::Auto {}
    impl Sealed for super::SumAuto {}
}

impl SearchType for Normal {
    type Output = Vec<NormalData>;

    async fn run(&self, client: &Client) -> Result<Self::Output> {
        let mut params = Params::dates(self.start_date, self.end_date, &self.community);
        params.set("page", self.page);
        params.set("pageSize", self.page_size);
        self.filters.append_to(&mut params);
        params.list("sort", &self.sort);
        client.public_get("/api-search", &params).await
    }
}

impl SearchType for Sum {
    type Output = Vec<SumData>;

    async fn run(&self, client: &Client) -> Result<Self::Output> {
        let mut params = Params::dates(self.start_date, self.end_date, &self.community);
        self.filters.append_to(&mut params);
        params.list("sort", &self.sort);
        client.public_get("/api-sum-search", &params).await
    }
}

impl SearchType for Auto {
    type Output = Vec<AutoData>;

    async fn run(&self, client: &Client) -> Result<Self::Output> {
        let mut params = Params::dates(self.start_date, self.end_date, &self.community);
        params.set("page", self.page);
        params.set("pageSize", self.page_size);
        self.filters.append_to(&mut params);
        params.list("sort", &self.sort);
        client.public_get("/api-search-auto", &params).await
    }
}

impl SearchType for SumAuto {
    type Output = Vec<SumAutoData>;

    async fn run(&self, client: &Client) -> Result<Self::Output> {
        let mut params = Params::dates(self.start_date, self.end_date, &self.community);
        self.filters.append_to(&mut params);
        client.public_get("/api-sum-search-auto", &params).await
    }
}

/// Largest page size the public search API accepts.
pub const MAX_PAGE_SIZE: u32 = 2000;

/// A search whose results are split into pages ([`Normal`] and [`Auto`]).
/// [`Client::search_all`] uses it to walk every page.
pub trait PagedSearch: SearchType<Output = Vec<Self::Item>> + Clone {
    /// The type of each result.
    type Item;

    /// Zero-based page this search requests.
    fn page(&self) -> u32;

    /// Changes the page this search requests.
    fn set_page(&mut self, page: u32);

    /// Number of results per page.
    fn page_size(&self) -> u32;

    /// Changes the number of results per page.
    fn set_page_size(&mut self, page_size: u32);
}

impl PagedSearch for Normal {
    type Item = NormalData;

    fn page(&self) -> u32 {
        self.page
    }

    fn set_page(&mut self, page: u32) {
        self.page = page;
    }

    fn page_size(&self) -> u32 {
        self.page_size
    }

    fn set_page_size(&mut self, page_size: u32) {
        self.page_size = page_size;
    }
}

impl PagedSearch for Auto {
    type Item = AutoData;

    fn page(&self) -> u32 {
        self.page
    }

    fn set_page(&mut self, page: u32) {
        self.page = page;
    }

    fn page_size(&self) -> u32 {
        self.page_size
    }

    fn set_page_size(&mut self, page_size: u32) {
        self.page_size = page_size;
    }
}

/// Calls `fetch` with consecutive page numbers, starting at `first_page`,
/// until a page holds fewer than `page_size` items, and returns every item.
async fn collect_pages<T, F, Fut>(first_page: u32, page_size: u32, mut fetch: F) -> Result<Vec<T>>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<Vec<T>>>,
{
    let mut items = Vec::new();
    let mut page = first_page;
    loop {
        let batch = fetch(page).await?;
        let last = batch.len() < page_size as usize;
        items.extend(batch);
        if last {
            return Ok(items);
        }
        page += 1;
    }
}

/// Empty query string, for endpoints that take no parameters.
const NO_QUERY: &[(&str, &str)] = &[];

impl Client {
    pub fn new(auth: Auth) -> Client {
        Client {
            auth,
            http: reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("could not initialize the HTTP client"),
            token: Mutex::new(None),
        }
    }

    /// Ensures the client holds a bearer token, authenticating against the
    /// Datadis login endpoint when there is none and returning the cached
    /// token otherwise.
    ///
    /// Every private endpoint method calls this, so callers never have to
    /// authenticate explicitly. When the API rejects a cached token as
    /// expired, the token is discarded and a fresh one is requested.
    pub async fn ensure_logged_in(&self) -> Result<String> {
        let mut token = self.token.lock().await;
        if let Some(token) = token.as_ref() {
            return Ok(token.clone());
        }
        let fresh = self.login().await?;
        *token = Some(fresh.clone());
        Ok(fresh)
    }

    /// Exchanges the configured credentials for a bearer token.
    async fn login(&self) -> Result<String> {
        let response = self
            .http
            .post(LOGIN_URL)
            .form(&[
                ("username", &self.auth.username),
                ("password", &self.auth.password),
            ])
            .send()
            .await?;
        if matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ) {
            return Err(Error::AuthenticationError);
        }
        Ok(response.error_for_status()?.text().await?)
    }

    /// Drops `stale` from the cache, unless another request already replaced it.
    async fn invalidate_token(&self, stale: &str) {
        let mut token = self.token.lock().await;
        if token.as_deref() == Some(stale) {
            *token = None;
        }
    }

    /// Performs an authenticated `GET` against the private API. If the cached
    /// token is rejected with `401 Unauthorized`, logs in again and retries
    /// once.
    async fn private_get<T, Q>(&self, path: &str, query: &Q) -> Result<T>
    where
        T: DeserializeOwned,
        Q: Serialize + ?Sized,
    {
        let url = format!("{BASE_URL}{path}");
        let mut retried = false;
        loop {
            let token = self.ensure_logged_in().await?;
            let response = self
                .http
                .get(&url)
                .bearer_auth(&token)
                .query(query)
                .send()
                .await?;
            if response.status() == StatusCode::UNAUTHORIZED && !retried {
                self.invalidate_token(&token).await;
                retried = true;
                continue;
            }
            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                return Err(Error::Api { status, body });
            }
            return Ok(response.json().await?);
        }
    }

    /// Performs an unauthenticated `GET` against the public search API.
    async fn public_get<T: DeserializeOwned>(&self, path: &str, params: &Params) -> Result<T> {
        Ok(self
            .http
            .get(format!("{PUBLIC_BASE_URL}{path}"))
            .query(&params.0)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    /// Datadis answers `404` with the body `No supplies` when the account has
    /// no supply points; that is reported as an empty list, not an error.
    pub async fn supplies(&self, query: SuppliesQuery) -> Result<Supplies> {
        match self.private_get("/get-supplies-v2", &query).await {
            Err(Error::Api { status, body })
                if status == StatusCode::NOT_FOUND && body.trim() == "No supplies" =>
            {
                Ok(Supplies {
                    supplies: Vec::new(),
                    distributor_error: Vec::new(),
                })
            }
            result => result,
        }
    }

    pub async fn contracts(&self, query: ContractsQuery) -> Result<Contracts> {
        self.private_get("/get-contract-detail-v2", &query).await
    }

    pub async fn consumption(&self, query: ConsumptionQuery) -> Result<Consumption> {
        self.private_get("/get-consumption-data-v2", &query).await
    }

    pub async fn max_power(&self, query: MaxPowerQuery) -> Result<MaxPowers> {
        self.private_get("/get-max-power-v2", &query).await
    }

    pub async fn distributors(&self, query: DistributorsQuery) -> Result<Distributors> {
        self.private_get("/get-distributors-with-supplies-v2", &query)
            .await
    }

    pub async fn reactive_data(&self, query: ReactiveDataQuery) -> Result<ReactiveData> {
        self.private_get("/get-reactive-data-v2", &query).await
    }

    /// `/get-groups-v2`. Not part of the published Datadis documentation.
    pub async fn groups(&self) -> Result<Vec<Group>> {
        self.private_get("/get-groups-v2", NO_QUERY).await
    }

    /// Returns the active partners registered in the platform.
    pub async fn partners(&self) -> Result<Vec<Partner>> {
        self.private_get("/partner-list", NO_QUERY).await
    }

    pub async fn new_authorization(
        &self,
        query: NewAuthorizationQuery,
    ) -> Result<AuthorizationResult> {
        let mut params: Vec<(&str, String)> = vec![("authorizedNif", query.authorized_nif.clone())];
        if let Some(start_date) = query.start_date {
            params.push(("startDate", dates::format_day(start_date)));
        }
        if let Some(end_date) = query.end_date {
            params.push(("endDate", dates::format_day(end_date)));
        }
        params.extend(query.cups.iter().map(|cups| ("cups", cups.clone())));
        self.private_get("/new-authorization", &params).await
    }

    pub async fn cancel_authorization(
        &self,
        query: CancelAuthorizationQuery,
    ) -> Result<AuthorizationResult> {
        let mut params: Vec<(&str, &str)> = vec![("authorizedNif", &query.authorized_nif)];
        params.extend(query.cups.iter().map(|cups| ("cups", cups.as_str())));
        self.private_get("/cancel-authorization", &params).await
    }

    pub async fn authorizations(&self, query: AuthorizationsQuery) -> Result<Vec<Authorization>> {
        self.private_get("/list-authorization", &query).await
    }

    /// Runs an aggregated search on the public API. These endpoints need no
    /// authentication, so this never logs in.
    pub async fn search<S: SearchType>(&self, search: S) -> Result<S::Output> {
        search.run(self).await
    }

    /// Runs a paged search and follows it page by page, starting at the
    /// search's `page`, until a page comes back with fewer than `page_size`
    /// results. Returns the results of every page fetched.
    ///
    /// A `page_size` of `0` is replaced by [`MAX_PAGE_SIZE`]. Use
    /// [`Client::search`] instead to fetch a single page.
    pub async fn search_all<S: PagedSearch>(&self, mut search: S) -> Result<Vec<S::Item>> {
        if search.page_size() == 0 {
            search.set_page_size(MAX_PAGE_SIZE);
        }
        collect_pages(search.page(), search.page_size(), |page| {
            let mut search = search.clone();
            search.set_page(page);
            async move { search.run(self).await }
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_documented_search_sample() {
        // Response sample from the `/api-search` documentation, where numbers
        // arrive as strings.
        let json = r#"[{
            "dataDay":11, "dataMonth":10, "dataYear":2021,
            "community":"Madrid, Comunidad de", "province":"", "municipality":"",
            "postalCode":"", "fare":"", "timeDiscrimination":"", "measurePointType":"",
            "sumEnergy":"57814623", "sumContracts":"3423561", "tension":"",
            "economicSector":"", "distributor":"",
            "mi1":"1964137", "mi2":"1792914", "mi24":"2239244", "mi25":"0"
        }]"#;
        let data: Vec<NormalData> = serde_json::from_str(json).unwrap();
        let item = &data[0];
        assert_eq!(
            (item.data_day, item.data_month, item.data_year),
            (11, 10, 2021)
        );
        assert_eq!(item.sum_contracts, 3_423_561);
        assert_eq!(item.sum_energy, 57_814_623.0);
        assert_eq!(item.hourly.len(), 25);
        assert_eq!(item.hourly[0], Some(1_964_137.0));
        assert_eq!(item.hourly[2], None);
        assert_eq!(item.hourly[24], Some(0.0));
    }

    #[test]
    fn deserializes_documented_search_auto_sample() {
        let json = r#"[{
            "dataDay":11, "dataMonth":10, "dataYear":2021,
            "community":"Madrid, Comunidad de", "province":"", "selfConsumption":"",
            "sumPower":"45939", "sumEnergy":"110498", "sumContracts":"7655",
            "distributor":"", "mi1":"1904", "mi13":"12024"
        }]"#;
        let data: Vec<AutoData> = serde_json::from_str(json).unwrap();
        assert_eq!(data[0].sum_power, 45_939.0);
        assert_eq!(data[0].sum_contracts, 7_655);
        assert_eq!(data[0].hourly[12], Some(12_024.0));
    }

    #[test]
    fn deserializes_documented_sum_samples() {
        let sum: Vec<SumData> =
            serde_json::from_str(r#"[{"sumEnergy":1863572529,"sumContract":3425358}]"#).unwrap();
        assert_eq!(sum[0].sum_contract, 3_425_358);

        let sum_auto: Vec<SumAutoData> = serde_json::from_str(
            r#"[{"sumEnergy":3175846,"sumPower":1450291,"sumContract":8277}]"#,
        )
        .unwrap();
        assert_eq!(sum_auto[0].sum_power, 1_450_291.0);
    }

    #[test]
    fn deserializes_documented_partner_sample() {
        let partners: Vec<Partner> = serde_json::from_str(
            r#"[{"name":"TEST","document":"99999999I","email":"testprueba@prueba.com",
                "registrationDate":"2026-03-11 00:00:00.0"}]"#,
        )
        .unwrap();
        assert_eq!(partners[0].document, "99999999I");
    }

    #[test]
    fn deserializes_authorization_without_cups() {
        let authorizations: Vec<Authorization> = serde_json::from_str(
            r#"[{"id":1,"ownerDocument":"A","requesterDocument":"B","status":"ACTIVE",
                "validityDateStart":"2024/01/01","validityDateEnd":"2025/01/01",
                "distributorCodeFather":"2"}]"#,
        )
        .unwrap();
        assert!(authorizations[0].cups.is_none());
    }

    #[test]
    fn builds_search_query_string() {
        let mut params = Params::dates(
            NaiveDate::from_ymd_opt(2020, 2, 1).unwrap(),
            NaiveDate::from_ymd_opt(2020, 2, 28).unwrap(),
            &[Community::Andalucia, Community::Madrid],
        );
        DatasetFilters {
            postal_code: vec!["18817".into(), "18800".into()],
            group_by_postal_code: Some(true),
            ..Default::default()
        }
        .append_to(&mut params);
        assert_eq!(
            params.0,
            vec![
                ("startDate", "2020/02/01".to_string()),
                ("endDate", "2020/02/28".to_string()),
                ("community", "01,13".to_string()),
                ("groupByPostalCode", "1".to_string()),
                ("postalCode", "18817,18800".to_string()),
            ]
        );
    }
}

#[cfg(test)]
mod paging_tests {
    use super::*;

    #[tokio::test]
    async fn follows_pages_until_a_short_page() {
        let mut requested = Vec::new();
        let items = collect_pages(3, 2, |page| {
            requested.push(page);
            async move {
                Ok(match page {
                    3 | 4 => vec![page, page],
                    _ => vec![page],
                })
            }
        })
        .await
        .unwrap();
        assert_eq!(requested, vec![3, 4, 5]);
        assert_eq!(items, vec![3, 3, 4, 4, 5]);
    }

    #[tokio::test]
    async fn stops_on_an_empty_page() {
        let items: Vec<u32> = collect_pages(0, 2, |page| async move {
            Ok(if page == 0 { vec![1, 2] } else { vec![] })
        })
        .await
        .unwrap();
        assert_eq!(items, vec![1, 2]);
    }

    #[test]
    fn maps_community_codes() {
        assert_eq!(Community::Madrid.code(), "13");
        assert_eq!(Community::from_code("09"), Some(Community::Cataluna));
        assert_eq!(Community::from_code("20"), None);
        for (index, community) in Community::ALL.iter().enumerate() {
            assert_eq!(community.code(), format!("{:02}", index + 1));
        }
    }
}

#[cfg(test)]
mod date_tests {
    use super::*;

    fn day(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn consumption_rolls_24_00_into_the_next_day() {
        let consumption: Consumption = serde_json::from_str(
            r#"{"timeCurve":[
                {"cups":"ES1","date":"2024/12/31","time":"01:00","consumptionKWh":0.2,"obtainMethod":"Real"},
                {"cups":"ES1","date":"2024/12/31","time":"24:00","consumptionKWh":0.3,"obtainMethod":"Real"}
            ]}"#,
        )
        .unwrap();
        let [first, last] = &consumption.time_curve[..] else {
            panic!("expected two items");
        };
        assert_eq!(
            first.datetime,
            day(2024, 12, 31).and_hms_opt(1, 0, 0).unwrap()
        );
        assert_eq!(last.date, day(2024, 12, 31));
        assert_eq!(last.datetime, day(2025, 1, 1).and_hms_opt(0, 0, 0).unwrap());
    }

    #[test]
    fn supply_without_end_date() {
        let supply: Supply = serde_json::from_str(
            r#"{"address":"A","cups":"ES1","postalCode":"28001","province":"Madrid",
                "municipality":"Madrid","distributor":"I-DE","validDateFrom":"2020/01/15",
                "validDateTo":"","pointType":5,"distributorCode":"8"}"#,
        )
        .unwrap();
        assert_eq!(supply.valid_date_from, day(2020, 1, 15));
        assert_eq!(supply.valid_date_to, None);
    }

    #[test]
    fn parses_months_and_partner_timestamps() {
        assert_eq!(dates::parse_month("2024/02").unwrap(), day(2024, 2, 1));
        let partner: Partner = serde_json::from_str(
            r#"{"name":"TEST","document":"99999999I","email":"e",
                "registrationDate":"2026-03-11 00:00:00.0"}"#,
        )
        .unwrap();
        assert_eq!(
            partner.registration_date,
            day(2026, 3, 11).and_hms_opt(0, 0, 0).unwrap()
        );
    }

    #[test]
    fn serializes_monthly_query_dates() {
        let query = ConsumptionQuery {
            cups: "ES1".into(),
            distributor_code: "2".into(),
            start_date: day(2024, 1, 15),
            end_date: day(2024, 3, 1),
            measurement_type: "0".into(),
            point_type: "5".into(),
            authorized_nif: None,
        };
        let value = serde_json::to_value(&query).unwrap();
        assert_eq!(value["startDate"], "2024/01");
        assert_eq!(value["endDate"], "2024/03");
    }

    #[test]
    fn search_rows_expose_their_date() {
        let row: NormalData = serde_json::from_str(
            r#"{"dataDay":11,"dataMonth":10,"dataYear":2021,"community":"","province":"",
                "municipality":"","postalCode":"","fare":"","timeDiscrimination":"",
                "measurePointType":"","sumEnergy":"1","sumContracts":"1","tension":"",
                "economicSector":"","distributor":""}"#,
        )
        .unwrap();
        assert_eq!(row.date(), Some(day(2021, 10, 11)));
    }
}
