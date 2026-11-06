//! Demo name tables: fixed, fictional, deterministic. Emails only on `.test`, phones only in the
//! demo's 0712…/0713…/0714… range.

/// US business customers: (company, city, state).
pub const BUSINESSES: &[(&str, &str, &str)] = &[
    ("Juniper Street Cafe", "Asheville", "NC"),
    ("Riverbend Natural Foods", "Greenville", "SC"),
    ("Hollow Oak Bakery", "Knoxville", "TN"),
    ("Saltmarsh Grocers", "Charleston", "SC"),
    ("Two Ravens Coffee Bar", "Durham", "NC"),
    ("Brightwater Co-op", "Roanoke", "VA"),
    ("Copper Kettle Tea Room", "Chattanooga", "TN"),
    ("Pine Needle Market", "Boone", "NC"),
    ("Lantern Hill Inn", "Blowing Rock", "NC"),
    ("Fieldstone Deli", "Spartanburg", "SC"),
    ("Morning Glory Bagels", "Raleigh", "NC"),
    ("Thistle & Thyme Kitchen", "Athens", "GA"),
    ("Old Mill Gift Shop", "Gatlinburg", "TN"),
    ("Kestrel Climbing Gym Cafe", "Asheville", "NC"),
    ("Granite Falls General Store", "Hickory", "NC"),
];

pub const US_FIRST: &[&str] = &[
    "Avery", "Jordan", "Morgan", "Riley", "Casey", "Taylor", "Quinn", "Harper", "Rowan", "Emerson",
    "Parker", "Reese", "Skyler", "Dakota", "Finley", "Hayden", "Logan", "Sawyer", "Blake",
    "Cameron", "Drew", "Elliot", "Jamie", "Kendall", "Marlowe",
];
pub const US_LAST: &[&str] = &[
    "Whitaker",
    "Lindqvist",
    "Okafor",
    "Delgado",
    "Brennan",
    "Haverford",
    "Nakamura",
    "Castillo",
    "Pemberton",
    "Ashworth",
    "Galloway",
    "Moreau",
    "Sinclair",
    "Holloway",
    "Vance",
    "Kowalski",
    "Fairbanks",
    "Thornton",
    "Abernathy",
    "Quintero",
    "Rasmussen",
    "Calloway",
    "Ellery",
    "Duvall",
    "Merriweather",
];
pub const US_STREETS: &[&str] = &[
    "Haywood Rd",
    "Lexington Ave",
    "Merrimon Ave",
    "Charlotte St",
    "Biltmore Ave",
    "Patton Ave",
    "Tunnel Rd",
    "Sweeten Creek Rd",
];
pub const US_CITIES: &[(&str, &str)] = &[
    ("Asheville", "NC"),
    ("Black Mountain", "NC"),
    ("Weaverville", "NC"),
    ("Hendersonville", "NC"),
    ("Brevard", "NC"),
    ("Waynesville", "NC"),
];

/// An extra village (invented), its officer and its farmers' phone prefix.
pub struct Village {
    pub code: &'static str,
    pub name: &'static str,
    pub lat: &'static str,
    pub lon: &'static str,
    /// Planned capacity in kW.
    pub kw: &'static str,
    /// The officer: (email, first name, last name).
    pub officer: (&'static str, &'static str, &'static str),
    pub phones: &'static str,
}

pub const VILLAGES: &[Village] = &[
    Village {
        code: "KITOWO",
        name: "Kitowo",
        lat: "-7.7460",
        lon: "35.7020",
        kw: "220",
        officer: ("rose@ruaha-demo.test", "Rose", "Mbilinyi"),
        phones: "0714001",
    },
    Village {
        code: "LUGODA",
        name: "Lugoda",
        lat: "-7.6840",
        lon: "35.6510",
        kw: "90",
        officer: ("daudi@ruaha-demo.test", "Daudi", "Kalinga"),
        phones: "0714002",
    },
    Village {
        code: "MWANGEZA",
        name: "Mwangeza",
        lat: "-7.8310",
        lon: "35.6930",
        kw: "40",
        officer: ("zuhura@ruaha-demo.test", "Zuhura", "Nyagawa"),
        phones: "0714003",
    },
];

/// A second ops user who is also field officer in Ilundo (two roles: the role picker).
pub const TWO_ROLES: (&str, &str, &str) = ("hamisi.ops@ruaha-demo.test", "Hamisi", "Chengula");
/// Mgama gets a second officer.
pub const MGAMA_OFFICER: (&str, &str, &str) = ("upendo@ruaha-demo.test", "Upendo", "Sanga");

pub const TZ_GIVEN: &[&str] = &[
    "Amani", "Baraka", "Chausiku", "Daudi", "Eliya", "Fatuma", "Gift", "Halima", "Imani", "Jabiri",
    "Kheri", "Lucy", "Mwanaidi", "Nuru", "Omari", "Pendo", "Rashidi", "Saida", "Tumaini", "Upendo",
    "Waziri", "Yusufu", "Zawadi", "Agnes", "Furaha", "Godfrey", "Hadija", "Issa", "Joyce",
    "Kassim",
];
pub const TZ_FAMILY: &[&str] = &[
    "Mkwawa",
    "Kihwele",
    "Lugenge",
    "Mgimwa",
    "Ngwale",
    "Sanga",
    "Mhenga",
    "Kilave",
    "Mbena",
    "Chusi",
    "Mlelwa",
    "Kitosi",
    "Mwamwindi",
    "Nyalusi",
    "Lyimo",
    "Mtewele",
    "Kipangula",
    "Chaula",
    "Mwalongo",
    "Kidamwina",
];

/// Survey titles: (en, sw, reward TZS minor).
pub const SURVEYS: &[(&str, &str, i64)] = &[
    ("Coffee picking labour", "Kazi ya kuchuma kahawa", 250_000),
    ("Cooking fuel", "Nishati ya kupikia", 200_000),
    ("Phone and mobile money", "Simu na pesa za simu", 150_000),
    ("Irrigation interest", "Nia ya umwagiliaji", 300_000),
    ("Seed sources", "Vyanzo vya mbegu", 150_000),
    ("Post-harvest losses", "Upotevu baada ya mavuno", 200_000),
    (
        "Village meeting attendance",
        "Mahudhurio ya mikutano",
        100_000,
    ),
    ("School fees timing", "Wakati wa ada za shule", 100_000),
];

/// Market buyers: (name, channel, note).
pub const BUYERS: &[(&str, &str, &str)] = &[
    (
        "Mafinga Grain Traders",
        "direct",
        "DEMO: pays on collection",
    ),
    ("Iringa Coffee Curing", "direct", "DEMO: wet parchment only"),
    (
        "Kilolo Sunflower Press",
        "other",
        "DEMO: oil press, collects at the road",
    ),
];
