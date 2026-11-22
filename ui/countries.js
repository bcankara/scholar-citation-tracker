"use strict";
/* ISO 3166-1 alpha-2 code -> [Turkish, English] names for the countries that appear most in
   scholarly output. Codes missing here fall back to Intl.DisplayNames, then to the bare code. */
(() => {
  const MAP = {
    AD: ["Andorra", "Andorra"], AE: ["Birleşik Arap Emirlikleri", "United Arab Emirates"], AF: ["Afganistan", "Afghanistan"],
    AL: ["Arnavutluk", "Albania"], AM: ["Ermenistan", "Armenia"], AO: ["Angola", "Angola"], AR: ["Arjantin", "Argentina"],
    AT: ["Avusturya", "Austria"], AU: ["Avustralya", "Australia"], AZ: ["Azerbaycan", "Azerbaijan"],
    BA: ["Bosna-Hersek", "Bosnia and Herzegovina"], BD: ["Bangladeş", "Bangladesh"], BE: ["Belçika", "Belgium"],
    BG: ["Bulgaristan", "Bulgaria"], BH: ["Bahreyn", "Bahrain"], BN: ["Brunei", "Brunei"], BO: ["Bolivya", "Bolivia"],
    BR: ["Brezilya", "Brazil"], BW: ["Botsvana", "Botswana"], BY: ["Belarus", "Belarus"],
    CA: ["Kanada", "Canada"], CH: ["İsviçre", "Switzerland"], CI: ["Fildişi Sahili", "Côte d'Ivoire"], CL: ["Şili", "Chile"],
    CM: ["Kamerun", "Cameroon"], CN: ["Çin", "China"], CO: ["Kolombiya", "Colombia"], CR: ["Kosta Rika", "Costa Rica"],
    CU: ["Küba", "Cuba"], CY: ["Kıbrıs", "Cyprus"], CZ: ["Çekya", "Czechia"],
    DE: ["Almanya", "Germany"], DK: ["Danimarka", "Denmark"], DO: ["Dominik Cumhuriyeti", "Dominican Republic"],
    DZ: ["Cezayir", "Algeria"], EC: ["Ekvador", "Ecuador"], EE: ["Estonya", "Estonia"], EG: ["Mısır", "Egypt"],
    ES: ["İspanya", "Spain"], ET: ["Etiyopya", "Ethiopia"], FI: ["Finlandiya", "Finland"], FR: ["Fransa", "France"],
    GB: ["Birleşik Krallık", "United Kingdom"], GE: ["Gürcistan", "Georgia"], GH: ["Gana", "Ghana"], GR: ["Yunanistan", "Greece"],
    GT: ["Guatemala", "Guatemala"], HK: ["Hong Kong", "Hong Kong"], HR: ["Hırvatistan", "Croatia"], HU: ["Macaristan", "Hungary"],
    ID: ["Endonezya", "Indonesia"], IE: ["İrlanda", "Ireland"], IL: ["İsrail", "Israel"], IN: ["Hindistan", "India"],
    IQ: ["Irak", "Iraq"], IR: ["İran", "Iran"], IS: ["İzlanda", "Iceland"], IT: ["İtalya", "Italy"],
    JM: ["Jamaika", "Jamaica"], JO: ["Ürdün", "Jordan"], JP: ["Japonya", "Japan"],
    KE: ["Kenya", "Kenya"], KG: ["Kırgızistan", "Kyrgyzstan"], KH: ["Kamboçya", "Cambodia"], KR: ["Güney Kore", "South Korea"],
    KW: ["Kuveyt", "Kuwait"], KZ: ["Kazakistan", "Kazakhstan"], LA: ["Laos", "Laos"], LB: ["Lübnan", "Lebanon"],
    LI: ["Lihtenştayn", "Liechtenstein"], LK: ["Sri Lanka", "Sri Lanka"], LT: ["Litvanya", "Lithuania"],
    LU: ["Lüksemburg", "Luxembourg"], LV: ["Letonya", "Latvia"], LY: ["Libya", "Libya"],
    MA: ["Fas", "Morocco"], MC: ["Monako", "Monaco"], MD: ["Moldova", "Moldova"], ME: ["Karadağ", "Montenegro"],
    MK: ["Kuzey Makedonya", "North Macedonia"], MM: ["Myanmar", "Myanmar"], MN: ["Moğolistan", "Mongolia"], MO: ["Makao", "Macao"],
    MT: ["Malta", "Malta"], MU: ["Mauritius", "Mauritius"], MX: ["Meksika", "Mexico"], MY: ["Malezya", "Malaysia"],
    MZ: ["Mozambik", "Mozambique"], NA: ["Namibya", "Namibia"], NG: ["Nijerya", "Nigeria"], NL: ["Hollanda", "Netherlands"],
    NO: ["Norveç", "Norway"], NP: ["Nepal", "Nepal"], NZ: ["Yeni Zelanda", "New Zealand"], OM: ["Umman", "Oman"],
    PA: ["Panama", "Panama"], PE: ["Peru", "Peru"], PH: ["Filipinler", "Philippines"], PK: ["Pakistan", "Pakistan"],
    PL: ["Polonya", "Poland"], PR: ["Porto Riko", "Puerto Rico"], PS: ["Filistin", "Palestine"], PT: ["Portekiz", "Portugal"],
    PY: ["Paraguay", "Paraguay"], QA: ["Katar", "Qatar"], RO: ["Romanya", "Romania"], RS: ["Sırbistan", "Serbia"],
    RU: ["Rusya", "Russia"], RW: ["Ruanda", "Rwanda"], SA: ["Suudi Arabistan", "Saudi Arabia"], SD: ["Sudan", "Sudan"],
    SE: ["İsveç", "Sweden"], SG: ["Singapur", "Singapore"], SI: ["Slovenya", "Slovenia"], SK: ["Slovakya", "Slovakia"],
    SM: ["San Marino", "San Marino"], SN: ["Senegal", "Senegal"], SY: ["Suriye", "Syria"],
    TH: ["Tayland", "Thailand"], TJ: ["Tacikistan", "Tajikistan"], TM: ["Türkmenistan", "Turkmenistan"], TN: ["Tunus", "Tunisia"],
    TR: ["Türkiye", "Türkiye"], TT: ["Trinidad ve Tobago", "Trinidad and Tobago"], TW: ["Tayvan", "Taiwan"], TZ: ["Tanzanya", "Tanzania"],
    UA: ["Ukrayna", "Ukraine"], UG: ["Uganda", "Uganda"], US: ["Amerika Birleşik Devletleri", "United States"],
    UY: ["Uruguay", "Uruguay"], UZ: ["Özbekistan", "Uzbekistan"], VA: ["Vatikan", "Vatican City"], VE: ["Venezuela", "Venezuela"],
    VN: ["Vietnam", "Vietnam"], XK: ["Kosova", "Kosovo"], YE: ["Yemen", "Yemen"], ZA: ["Güney Afrika", "South Africa"],
    ZM: ["Zambiya", "Zambia"], ZW: ["Zimbabve", "Zimbabwe"],
  };

  /** Localized country name for an ISO code; `lang` is "tr" or "en". */
  function countryName(code, lang) {
    const c = String(code || "").toUpperCase();
    const hit = MAP[c];
    if (hit) return lang === "tr" ? hit[0] : hit[1];
    try {
      const n = new Intl.DisplayNames([lang === "tr" ? "tr" : "en"], { type: "region" }).of(c);
      if (n && n !== c) return n;
    } catch { /* unknown code: fall through */ }
    return c;
  }

  window.COUNTRIES = { map: MAP, name: countryName };
})();
