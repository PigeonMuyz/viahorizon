// SPDX-License-Identifier: GPL-3.0-or-later
// Generated from Mojang version.json data; see tools/data/minecraft-data-versions.json.
// 1.19.3, version.json SHA256: 0569733ffcddd57f8cf445cac890aafc33dc4b78a0fff491e741850f1a6b6b58
pub const P761: i32 = 3218;
// 1.19.4, version.json SHA256: 01719dd5c45137a6d88c860798ad267f9356e44e843bf8446f9393e59c223243
pub const P762: i32 = 3337;
// 1.20, version.json SHA256: edfc19fa9cc89331baaa4f5b9a28cae18bb5451a21df2bdf2656357bf0c64b28
pub const P763: i32 = 3463;
// 1.20.2, version.json SHA256: cea041350442b0e0215ba079f75aac155fa77cff51e10d6e0d0767a319b2d07e
pub const P764: i32 = 3578;
// 1.20.3, version.json SHA256: e05a48fe7476b3ac45177b82b6f662c15aae9559482384683a7a3899a3720096
pub const P765: i32 = 3698;
// 1.20.5, version.json SHA256: f3d9c385759a584be9eeb3816365e7f391ed56c290e784ffda4b8e133df145c7
pub const P766: i32 = 3837;
// 1.21, version.json SHA256: d563fa6ca9deeb60088232d48f07269fe8843633eaa82f0724e69d8465580e16
pub const P767: i32 = 3953;
// 1.21.2, version.json SHA256: c47d3034be76a7eab3c38c85d197f2a6855600ea534bc6cb16ca5123a4c57df3
pub const P768: i32 = 4080;
// 1.21.4, version.json SHA256: bd58f18fdd7000b756e44045c9ef7f10ba9f08113ac4ba1c877dc8f33e724cc3
pub const P769: i32 = 4189;
// 1.21.5, version.json SHA256: 24a31239d43575a4f98f0d2475f5004c3492b074f51f5e3749c46d08ebfb4bc9
pub const P770: i32 = 4325;
// 1.21.6, version.json SHA256: bf43fa33475b31b21981752d718a66620a53f66e0388e8b2bbc260ee6e0fcbbf
pub const P771: i32 = 4435;
// 1.21.7, version.json SHA256: 852db51ca90919d7371c2e8dba25457e3937540a17c7ad67f390485af7b9c9e3
pub const P772: i32 = 4438;
// 1.21.9, version.json SHA256: 2d30ee11695f14cde0f32ed1ff51e9e4700ae259ef4b04296e12373faede5c1b
pub const P773: i32 = 4554;
// 1.21.11, version.json SHA256: c0eccc5a56152ae84d999efeed146ba6a5ea378c4ccaecb03eceb06dbeb2a212
pub const P774: i32 = 4671;
// 26.1, version.json SHA256: ab17ef6006f8424581577e216a876a69bb8423a38fcc34749f011495115bc79c
pub const P775: i32 = 4786;
// 26.2, version.json SHA256: 8e6c534e0b430508190bc6ac397be6cb5c93d696300f20e225eda9f59d6fa021
pub const P776: i32 = 4903;
// 26.3, version.json SHA256: 70444a856374443fbd57e4b23e9acfdd50309b25906ced3e3cf658abf25dd3ae
pub const P777: i32 = 5023;
pub const fn for_protocol(protocol: i32) -> Option<i32> {
    match protocol {
        761 => Some(P761),
        762 => Some(P762),
        763 => Some(P763),
        764 => Some(P764),
        765 => Some(P765),
        766 => Some(P766),
        767 => Some(P767),
        768 => Some(P768),
        769 => Some(P769),
        770 => Some(P770),
        771 => Some(P771),
        772 => Some(P772),
        773 => Some(P773),
        774 => Some(P774),
        775 => Some(P775),
        776 => Some(P776),
        777 => Some(P777),
        _ => None,
    }
}
