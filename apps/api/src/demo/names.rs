//! Demo name tables: fixed, fictional, deterministic. Emails only on `.test`, phones only in the
//! demo's 0712…/0713…/0714… range.

/// Honey varietals: (name, brand, 8 oz price in cents). Each gives a jar (with sizes), a creamed
/// and a comb product.
pub const VARIETALS: &[(&str, &str, i64)] = &[
    ("Wildflower", "Blue Ridge Apiary", 899),
    ("Clover", "Blue Ridge Apiary", 849),
    ("Orange Blossom", "Cedar Hollow Bees", 999),
    ("Buckwheat", "Cedar Hollow Bees", 1049),
    ("Tupelo", "Cedar Hollow Bees", 1599),
    ("Sourwood", "Blue Ridge Apiary", 1499),
    ("Sage", "Hive & Hearth", 1199),
    ("Alfalfa", "Hive & Hearth", 849),
    ("Blueberry", "Blue Ridge Apiary", 1099),
    ("Basswood", "Hive & Hearth", 949),
    ("Avocado", "Cedar Hollow Bees", 1149),
    ("Tallow Tree", "Hive & Hearth", 999),
];

/// Jar sizes as (option value, price factor in percent of the 8 oz price).
pub const JAR_SIZES: &[(&str, i64)] = &[("8 oz", 100), ("16 oz", 180), ("32 oz", 320)];

/// Candle products with scent variants: (name, brand, price in cents).
pub const CANDLES: &[(&str, &str, i64)] = &[
    ("Beeswax pillar candle 3x4", "Meadowlark Candle Works", 2400),
    ("Beeswax taper pair 10 in", "Meadowlark Candle Works", 1600),
    (
        "Beeswax tea lights, 12 pack",
        "Meadowlark Candle Works",
        1800,
    ),
    ("Rolled beeswax candle", "Meadowlark Candle Works", 1200),
];
pub const SCENTS: &[&str] = &["Natural", "Lavender", "Cedar"];

/// Everything else, one product each: (name, brand, price in cents, store only, description).
/// An empty description is on purpose (the shop's "no description" state).
pub const OTHERS: &[(&str, &str, i64, bool, &str)] = &[
    (
        "Honey lip balm",
        "Hive & Hearth",
        450,
        false,
        "Beeswax, sunflower oil and a touch of raw honey.",
    ),
    (
        "Peppermint lip balm",
        "Hive & Hearth",
        450,
        false,
        "Beeswax and peppermint oil.",
    ),
    ("Unscented lip balm", "Hive & Hearth", 450, false, ""),
    (
        "Honey oat soap bar",
        "Hive & Hearth",
        750,
        false,
        "Cold-process soap with oats and honey.",
    ),
    (
        "Lavender honey soap bar",
        "Hive & Hearth",
        750,
        false,
        "Cold-process soap with lavender and honey.",
    ),
    (
        "Beeswax hand salve 2 oz",
        "Hive & Hearth",
        1250,
        false,
        "For dry, working hands.",
    ),
    (
        "Beeswax food wraps, 3 pack",
        "Meadowlark Candle Works",
        1800,
        false,
        "Reusable cotton wraps.",
    ),
    (
        "Pure beeswax block 1 lb",
        "Meadowlark Candle Works",
        1400,
        false,
        "Filtered yellow beeswax for crafts.",
    ),
    (
        "Honey dipper, maple",
        "Hive & Hearth",
        600,
        false,
        "Turned maple wood.",
    ),
    (
        "Honey stix, 20 pack",
        "Blue Ridge Apiary",
        500,
        false,
        "Clover honey in paper straws.",
    ),
    (
        "Bee pollen 8 oz",
        "Blue Ridge Apiary",
        1500,
        false,
        "Dried, cleaned pollen granules.",
    ),
    (
        "Propolis tincture 1 oz",
        "Cedar Hollow Bees",
        1700,
        false,
        "Alcohol extract of propolis.",
    ),
    (
        "Royal jelly 1 oz",
        "Cedar Hollow Bees",
        2900,
        false,
        "Kept frozen.",
    ),
    (
        "Hot honey 12 oz",
        "Blue Ridge Apiary",
        1299,
        false,
        "Wildflower honey infused with chili.",
    ),
    (
        "Cinnamon creamed spread 12 oz",
        "Blue Ridge Apiary",
        1199,
        false,
        "Creamed clover honey with cinnamon.",
    ),
    (
        "Honey mustard 8 oz",
        "Hive & Hearth",
        799,
        false,
        "Stone-ground, sweetened with honey.",
    ),
    (
        "Honey vinegar 12 oz",
        "Hive & Hearth",
        1399,
        false,
        "Aged honey vinegar.",
    ),
    ("Mead tasting glass", "Hive & Hearth", 900, false, ""),
    (
        "Chamomile tea, 20 bags",
        "Hive & Hearth",
        899,
        false,
        "Pairs well with clover honey.",
    ),
    (
        "Black tea with honey, 20 bags",
        "Hive & Hearth",
        899,
        false,
        "Assam with honey granules.",
    ),
    (
        "Ruaha coffee drip bags, 10 pack",
        "Ruaha Coffee Co-op",
        1450,
        false,
        "Single-cup filters, medium roast.",
    ),
    (
        "Ruaha coffee 500 g, ground",
        "Ruaha Coffee Co-op",
        2650,
        false,
        "Medium grind for drip and pour-over.",
    ),
    (
        "Ruaha coffee 1 kg, whole bean",
        "Ruaha Coffee Co-op",
        4900,
        false,
        "Arabica from smallholders in Tanzania.",
    ),
    (
        "Honey tasting flight, 4 jars",
        "Blue Ridge Apiary",
        3200,
        false,
        "Four 2 oz jars of seasonal honey.",
    ),
    (
        "Gift box: breakfast",
        "Blue Ridge Apiary",
        4500,
        false,
        "Honey, coffee and a dipper.",
    ),
    (
        "Gift box: tea time",
        "Hive & Hearth",
        3900,
        false,
        "Two teas, creamed honey and a dipper.",
    ),
    (
        "Gift box: candle lover",
        "Meadowlark Candle Works",
        5200,
        false,
        "Pillar, tapers and tea lights.",
    ),
    (
        "Gift wrap",
        "Hive & Hearth",
        300,
        true,
        "Added at the counter.",
    ),
    (
        "Tote bag, canvas",
        "Hive & Hearth",
        1500,
        true,
        "Printed with the shop's bee.",
    ),
    (
        "Honey by the pound (bring your jar)",
        "Blue Ridge Apiary",
        1100,
        true,
        "Weighed at the counter.",
    ),
    ("Local honey sample cup", "Blue Ridge Apiary", 100, true, ""),
];

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
