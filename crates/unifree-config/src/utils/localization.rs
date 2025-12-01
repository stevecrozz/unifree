use rust_iso3166::from_alpha2;

pub fn get_numeric_country_code(iso: &str) -> u16 {
    // rust_iso3166::from_alpha2 returns Option<Country>
    if let Some(c) = from_alpha2(iso) {
        c.numeric as u16
    } else {
        840 // Default US
    }
}

pub fn iana_to_posix_tz(iana: &str) -> Option<&'static str> {
    match iana {
        // Europe
        "Europe/Berlin" | "Europe/Amsterdam" | "Europe/Stockholm" | "Europe/Vienna"
        | "Europe/Zurich" | "Europe/Copenhagen" | "Europe/Oslo" | "Europe/Paris"
        | "Europe/Rome" | "Europe/Warsaw" | "Europe/Madrid" | "Europe/Brussels"
        | "Europe/Budapest" | "Europe/Prague" | "Europe/Belgrade" => {
            Some("CET-1CEST,M3.5.0,M10.5.0/3")
        }
        "Europe/London" | "Europe/Dublin" | "Europe/Lisbon" => Some("GMT0BST,M3.5.0/1,M10.5.0"),
        "Europe/Helsinki" | "Europe/Athens" | "Europe/Bucharest" | "Europe/Istanbul"
        | "Europe/Kiev" | "Europe/Riga" | "Europe/Sofia" | "Europe/Tallinn" | "Europe/Vilnius" => {
            Some("EET-2EEST,M3.5.0/3,M10.5.0/4")
        }
        "Europe/Moscow" => Some("MSK-3"),

        // North America
        "America/New_York" | "America/Detroit" | "America/Toronto" | "America/Montreal" => {
            Some("EST5EDT,M3.2.0,M11.1.0")
        }
        "America/Chicago" | "America/Winnipeg" | "America/Mexico_City" => {
            Some("CST6CDT,M3.2.0,M11.1.0")
        }
        "America/Denver" | "America/Edmonton" => Some("MST7MDT,M3.2.0,M11.1.0"),
        "America/Los_Angeles" | "America/Vancouver" | "America/Tijuana" => {
            Some("PST8PDT,M3.2.0,M11.1.0")
        }
        "America/Phoenix" => Some("MST7"),
        "America/Anchorage" => Some("AKST9AKDT,M3.2.0,M11.1.0"),
        "Pacific/Honolulu" => Some("HST10"),

        // South America
        "America/Sao_Paulo" => Some("BRT3"), // DST rules vary
        "America/Argentina/Buenos_Aires" => Some("ART3"),

        // Asia/Pacific
        "Asia/Tokyo" => Some("JST-9"),
        "Asia/Shanghai" | "Asia/Hong_Kong" | "Asia/Singapore" => Some("CST-8"),
        "Asia/Taipei" => Some("CST-8"),
        "Asia/Seoul" => Some("KST-9"),
        "Asia/Bangkok" | "Asia/Jakarta" | "Asia/Ho_Chi_Minh" => Some("ICT-7"),
        "Asia/Kolkata" => Some("IST-5:30"),
        "Australia/Sydney" | "Australia/Melbourne" => Some("AEST-10AEDT,M10.1.0,M4.1.0/3"),
        "Australia/Brisbane" => Some("AEST-10"),
        "Australia/Adelaide" => Some("ACST-9:30ACDT,M10.1.0,M4.1.0/3"),
        "Australia/Perth" => Some("AWST-8"),
        "Pacific/Auckland" => Some("NZST-12NZDT,M9.5.0,M4.1.0/3"),

        // UTC
        "UTC" | "Etc/UTC" | "GMT" => Some("UTC0"),

        _ => None,
    }
}

pub fn get_default_timezone_for_country(iso: &str) -> &'static str {
    match iso.to_uppercase().as_str() {
        "DE" | "AT" | "CH" | "NL" | "BE" | "LU" | "FR" | "ES" | "IT" | "PL" | "CZ" | "DK"
        | "NO" | "SE" => "Europe/Berlin",
        "GB" | "IE" | "PT" => "Europe/London",
        "FI" | "GR" | "EE" | "LV" | "LT" | "RO" | "BG" | "UA" | "TR" => "Europe/Helsinki",
        "RU" => "Europe/Moscow",
        "US" => "America/New_York", // Broad default
        "CA" => "America/Toronto",
        "AU" => "Australia/Sydney",
        "NZ" => "Pacific/Auckland",
        "JP" => "Asia/Tokyo",
        "CN" => "Asia/Shanghai",
        "IN" => "Asia/Kolkata",
        "BR" => "America/Sao_Paulo",
        _ => "UTC",
    }
}

/// Try to detect the system timezone using iana-time-zone crate
pub fn detect_system_timezone() -> Option<String> {
    iana_time_zone::get_timezone().ok()
}
