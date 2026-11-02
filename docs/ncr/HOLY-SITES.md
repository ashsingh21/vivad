# Holy sites on the NCR map

Reviewed on 1 Nov 2026. The list itself lives in `HOLY` in `tools/osm/ncr.py`. This page says where
each site came from and how far to trust it. The four old maps keep their own list in
`crates/shared/src/map/delhi.rs`; nothing here changes them.

A holy site stays standing as a plain building. What the list does:

- No chest or floor loot lands inside its circle, and nothing a chest or a floor bow drops lands
  there either (`no_drop_lands_at_a_holy_site`).
- No zone ring is centred within 120 m of the circle's edge (`HOLY_CLEAR` in `sim/zone.rs`).
- The game invents nothing. A circle marks a real place, and it is never named on screen.

A wrong circle costs a little loot. A missing one puts loot in a temple. So weak sites stay in, and
are marked as doubts here.

## Where it stands

| District | Sites | Sure | Likely | Doubt | Note |
|---|---|---|---|---|---|
| Karol Bagh | 12 | 3 | 8 | 1 | Dense market lanes, more likely unlisted |
| Laxmi Nagar | 15 | 9 | 4 | 2 | Dense, more likely unlisted |
| Pari Chowk | 6 | 2 | 4 | 0 | One strip of small temples |
| Saket | 4 | 4 | 0 | 0 | Qabristan inside the district centre |
| Indirapuram | 3 | 2 | 1 | 0 | Temples inside housing societies |
| Sector 18 | 3 | 2 | 0 | 1 |  |
| Film City | 3 | 2 | 0 | 1 | One is a memorial park's corner |
| Ghaziabad | 2 | 0 | 1 | 1 | Nehru Nagar, not the old town |
| Nehru Place | 2 | 0 | 0 | 2 | Both are doubts |
| Golf Course Road | 2 | 0 | 0 | 2 | Both are doubts |
| Anand Vihar | 1 | 0 | 0 | 1 | Railway and bus ground |
| Cyber City | 0 | 0 | 0 | 0 | None found |
| MG Road | 0 | 0 | 0 | 0 | None found |
| Expressway | 0 | 0 | 0 | 0 | None found |
| Aerocity | 0 | 0 | 0 | 0 | None found |
| Janakpuri | 1 | 0 | 1 | 0 | Not in `PLACES` |
| Vaishali | 3 | 2 | 1 | 0 | Not in `PLACES` |
| **In `PLACES`** | **53** | **24** | **18** | **11** | |

The partial work wrote 26 circles from OpenStreetMap alone. There are now 57, and 53 of those
are in districts `compose.rs` uses. Janakpuri and Vaishali are not in `PLACES`, so they had a lighter check.

**Ghaziabad is not the old town.** The square centred on 28.6680, 77.4400 is Nehru Nagar II, beside
Kavi Nagar, and the web map shows two temple pins inside it and no mosque. The old town's mosques
(Dasna Gate, Qureshi Market, Mirza Jaan) start about 1.2 km west of the middle of the square. So the
low count is believable for this ground. To show the old town, the district's centre has to move,
and its sites then need a new review.

**Zero is a result, not a gap,** for Cyber City, MG Road, Expressway and Aerocity. All four are
planned office or hotel ground. Five sources found no real site inside them, and separate web map
searches for temples, mosques, gurdwaras and churches showed only junk listings there. Nobody has
walked them.

## How the list was made

| Source | Date | What it gave | Positions used? |
|---|---|---|---|
| OpenStreetMap, Geofabrik Northern and Central Zone extracts in `build/osm` | 31 Oct 2026 | Scanned in full, not only the district cuts: 87 hits by tag or name over all 17 squares, most of them shops and bus routes | Yes |
| Overture Places, release `2026-10-21.0` | 21 Oct 2026 | 320,050 places in the NCR box. Real sites, mixed with wrong pins and wrong categories | Yes, the rows here are Meta data, CDLA Permissive 2.0 |
| Business addresses in Overture | 21 Oct 2026 | Text like "opp. Sai Mandir" points to sites with no pin of their own | Only the businesses' pins, to find the middle of a site's neighbours |
| Google Maps, read by eye, one screen a square at zoom 17. Separate searches on every square for temples, mosques, gurdwaras, churches and any place of worship, and for cremation grounds around five parts of the map | 1 Nov 2026 | The best source: real local sites by name and type | **No.** See below |
| Wikimapia, public API | 1 Nov 2026 | A few confirmations, such as Saket's Qabristan | **No.** CC BY-SA 3.0, which cannot mix into ODbL data |
| Bhakti Bharat temple lists by metro station | 1 Nov 2026 | Names, rarely with an address | No, leads only |
| Delhi Waqf Board managing committee list | 1 Nov 2026 | Two Karol Bagh mosques, both outside the square. It is a short list, not the full register | No, leads only |
| Wikidata and GeoNames | 1 Nov 2026 | Nothing new inside a square | Not used |

### The rule for positions

`ncr_*.json` are ODbL data, so anything put in them must be free to share under ODbL. Google's terms
forbid copying from its maps, and Wikimapia's share-alike licence does not fit. So the Position
column means:

- **osm**: OpenStreetMap's own point or outline.
- **overture**: Overture's pin.
- **placed**: our own choice of a point on an OpenStreetMap building or road beside the site.

Be clear on what "placed" is. No coordinate was copied, but a web map did tell us the site exists and
roughly where. Plus codes shown in addresses, and a few coordinates in page links, were used only to
decide whether a site falls inside a square. A `placed` point is about 20 to 40 m from the real site,
so its circle is drawn wider.

**This is a judgement call for the owner.** It should be fine for a modest list of facts about where
temples are. If you want the data free of any web map influence, delete the rows marked `placed` and
run the command at the end of this page: 25 of the 53 rows go, and those sites get loot again.

### The rule for radius

The site plus about 20 m, as on the old maps, and never under 25 m. An Overture pin gets at least
30 m and a placed point 30 to 50 m. A site goes in the file only if its circle reaches the district's
500 m square, because nothing outside that square exists in the game.

### How sure

- **sure**: two sources agree on the site.
- **likely**: one good source, such as a named site with reviews.
- **doubt**: one weak source. Kept to be safe.

## The sites

Position x, z is in metres from the district's middle, east and south. r is the radius.

### Karol Bagh

| Site | x, z | r | Position | Sure | Found in |
|---|---|---|---|---|---|
| Arya Samaj Mandir, 13/39 Arya Samaj Road | 153, -238 | 30 | overture | sure | Overture (confidence 0.93). The street is named after it |
| Shree Shakti Peeth Jai Mata Mandir, Padam Singh Road | 58, -168 | 30 | overture | likely | Overture, address near Mahajan Plaza |
| Shiv Mandir, Gurudwara Road | 187, 73 | 30 | overture | likely | Overture only |
| Sai Baba Ashram | -2, -21 | 30 | overture | likely | Overture only |
| Ramakrishna Ashram Kutir | -142, 65 | 25 | osm | likely | OpenStreetMap, tagged an NGO and Hindu. May be a hostel more than a shrine |
| War memorial, no name mapped | 238, 115 | 25 | osm | likely | OpenStreetMap, `memorial=war_memorial`, beside Guru Nanak Dev Park |
| Radhasoami Satsang Dayalbagh, 12/6 Padam Singh Road | -78, -267 | 30 | overture | sure | Overture and web map. Overture calls it Christian, which is wrong |
| Aarya Samaj Mandir, 26/5 Ajmal Khan Road | 21, -46 | 30 | placed | likely | Web map, 11 reviews. Could be the same body as the Arya Samaj Road one; both kept |
| Shree Hanuman Mandir, Ajmal Khan Road | -56, 163 | 45 | placed | sure | Web map (67 reviews), Wikimapia, and a shop next door gives its address as opposite it |
| Shiv Temple, by Maharana Pratap Road | -231, 203 | 40 | placed | likely | Web map, 3 reviews |
| Karumariamman Temple, Abdul Aziz Road | 252, 126 | 35 | placed | likely | Web map, 9 reviews |
| Karol Bagh Pooja Park, block 15A | -102, 99 | 35 | placed | doubt | Web map, 92 reviews. Looks like a yearly Durga Puja park, not a fixed shrine |

### Laxmi Nagar

| Site | x, z | r | Position | Sure | Found in |
|---|---|---|---|---|---|
| Buddha Mandir (Bodh Ashram), J-100 | -148, -221 | 59 | osm | sure | OpenStreetMap outline, Overture, Wikimapia and web map |
| Vishal Shakti Mandir | -190, -252 | 35 | osm | sure | OpenStreetMap (a point with no name), Overture and web map give the name |
| Gurudwara Shri Guru Singh Sabha, Lalita Park | -223, 135 | 30 | osm | sure | OpenStreetMap and Overture. Overture calls it Christian, which is wrong |
| Shri Shiv Mandir | -183, 110 | 25 | osm | likely | OpenStreetMap only |
| Pracheen Durga Mandir | -154, 219 | 45 | osm | sure | OpenStreetMap outline. The web map may call this Sheetla Mata Mandir |
| Manokamana Siddh Durga Mandir | -105, 230 | 41 | osm | sure | OpenStreetMap, Overture, Wikimapia and web map |
| Sharon Church of Christ and Hanuman Temple, D-264 to D-267 | -152, -109 | 50 | overture | sure | Overture and web map (19 reviews) for the church; web map only for the temple, three doors along. One circle covers both |
| Shree Pancheshwar Shiv Mandir | 195, -198 | 30 | overture | likely | Overture, at the spot where the web map shows a Shiv Mandir |
| Upasana Mandir, Vikas Marg | 90, -87 | 45 | overture | likely | Web map and Bhakti Bharat name it. The position is an Overture pin mislabelled Akshardam, 32 m away |
| Satnami Sadh Ashram, S-501 School Block | -86, 87 | 30 | overture | likely | Overture only |
| Shri Sai Baba Mandir, Vikas Marg | 165, -172 | 45 | placed | sure | Web map (62 reviews). A dozen business addresses say next to or opposite it; the point is the middle of those businesses |
| Shree Ram Mandir, Ram Mandir Gali | 53, 15 | 45 | placed | sure | Web map, a lane named after it, and business addresses. Shiv Shakti Mandir, listed near Patel Hospital, is probably inside this circle too |
| Gurudwara Sri Guru Singh Sabha, Gurudwara Street | 18, 79 | 35 | placed | sure | Web map (7 reviews). OpenStreetMap names the lane Gurudwara Street, and shops on it give that address. Only the single-faith search found it |
| Puncheswar Temple, a second pin for the Pancheshwar name | 128, -132 | 30 | placed | doubt | Web map, 100 m from Overture's pin of nearly the same name. May be a duplicate |
| Chatri Wala Shiv Goraksh Durgesh Mandir | 204, 159 | 35 | placed | doubt | Web map only, 1 review |

### Pari Chowk

| Site | x, z | r | Position | Sure | Found in |
|---|---|---|---|---|---|
| Shiv Parvati Mandir | 15, 71 | 40 | overture | sure | Overture and web map |
| Jhandewalan Temple | 37, 20 | 40 | placed | likely | Web map, 30 reviews |
| PanchMukhi Hanuman Mandir | 82, 72 | 35 | placed | likely | Web map, 2 reviews |
| Shree Ram Mandir | 8, 136 | 40 | placed | likely | Web map, 4 reviews |
| Jhande Wale Baba Mandir and the temple beside it | -30, 154 | 45 | placed | sure | Web map (21 and 12 reviews) and Wikimapia |
| Temple on Shiv Mandir Road, Tugalpur | -129, -167 | 40 | placed | likely | Web map (2 reviews); business addresses name the road |

Six circles along one strip south of the expressway. OpenStreetMap has none of these and Overture has
one. It is the biggest gain from reading the web map.

### Saket

| Site | x, z | r | Position | Sure | Found in |
|---|---|---|---|---|---|
| Qabristan | -42, 167 | 105 | osm | sure | OpenStreetMap outline (`landuse=cemetery`, no name); Wikimapia names it Qabristan |
| Prachin Shiv Shakti Durga Mandir | 63, -222 | 25 | osm | sure | OpenStreetMap and web map |
| Hanuman Mandir, Khirki | -56, -274 | 25 | osm | sure | OpenStreetMap, Wikimapia and web map |
| Sai Baba Temple, Khirki Main Road | -65, -293 | 48 | osm | sure | OpenStreetMap, Overture and Wikimapia |

Saket keeps only 180 m from its middle, so the three Khirki sites are past its edge in the game.

### Indirapuram

| Site | x, z | r | Position | Sure | Found in |
|---|---|---|---|---|---|
| Shiv Shakti Temple, Dr Sushila Naiyar Marg | 171, -80 | 33 | osm | sure | OpenStreetMap (building, no name); web map gives the name |
| Aditya Mega City Temple | -194, -36 | 35 | placed | sure | Web map, 11 reviews, inside the society |
| Krishna Apra Sapphire Temple | 162, 52 | 40 | placed | likely | Web map, 7 reviews, inside the society |

### Sector 18

| Site | x, z | r | Position | Sure | Found in |
|---|---|---|---|---|---|
| Gurudwara Sri Guru Singh Sabha, Sector 18 | 8, 223 | 64 | osm | sure | OpenStreetMap outline, Overture, Wikimapia, web map (2,219 reviews) and Bhakti Bharat |
| Shiv Shani Mandir | -275, 81 | 30 | overture | sure | Overture, Wikimapia and web map (73 reviews) |
| Sai Temple | -118, 121 | 30 | overture | doubt | Overture only. The web map's Sai Temple is about 1.7 km north, so this pin may be wrong |

### Film City

| Site | x, z | r | Position | Sure | Found in |
|---|---|---|---|---|---|
| Giant Shiv statue | 172, 244 | 50 | placed | sure | Web map (14 reviews) and two Wikimapia entries. A web search found nothing more |
| Rashtriya Dalit Prerna Sthal, its east tip | -235, 183 | 95 | osm | sure | OpenStreetMap outline and Wikidata. A state memorial park, not a religious site. The circle covers only the corner inside the square |
| Shiv Nandi, Express Trade Tower | -123, 47 | 30 | overture | doubt | Overture only. Could be an office with the wrong category |

Film City keeps only 200 m from its middle, so the statue's circle only just reaches the kept ground.

### Ghaziabad

| Site | x, z | r | Position | Sure | Found in |
|---|---|---|---|---|---|
| Shri Vedmata Gayatri Dhaam, Basant Road | -182, -207 | 45 | placed | likely | Web map only, 2 reviews |
| Gopal, a temple pin with no other detail | -143, 176 | 30 | placed | doubt | Web map only: a temple symbol, a one-word name, nothing else |

### Nehru Place

| Site | x, z | r | Position | Sure | Found in |
|---|---|---|---|---|---|
| Masjid, one listing with no reviews | -1, 71 | 30 | placed | doubt | Web map only, one listing, no reviews. A web search confirmed nothing |
| Nishchhal Temple, 33 Nehru Place Road | -134, -63 | 35 | placed | doubt | Web map only, no reviews and an odd full name |

### Golf Course Road

| Site | x, z | r | Position | Sure | Found in |
|---|---|---|---|---|---|
| Temple at Jawahar Apartment-1, GH-54 | -61, 17 | 40 | placed | doubt | Web map only, no reviews. A temple inside a housing society |
| Technical Paradise Temple | -265, 195 | 40 | placed | doubt | Web map only, a temple inside a housing society |

### Anand Vihar

| Site | x, z | r | Position | Sure | Found in |
|---|---|---|---|---|---|
| Small temple by the ISBT, its listing has a nonsense name | 52, 113 | 45 | placed | doubt | Web map only: typed Hindu temple, 12 reviews, named "Hona chahiye to be in" |

### Not in `PLACES` yet

Janakpuri:

| Site | x, z | r | Position | Sure | Found in |
|---|---|---|---|---|---|
| Shri Hare Ram Hanuman Mandir, Major Deepak Tyagi Marg | -210, 228 | 30 | overture | likely | Overture only. No web map check |

Vaishali:

| Site | x, z | r | Position | Sure | Found in |
|---|---|---|---|---|---|
| Shri Radhakrishna Mandir, Sector 4 | -95, 136 | 32 | osm | sure | OpenStreetMap and Overture. No web map check |
| Gopeshwar Mahadev Mandir | 202, 167 | 45 | osm | sure | OpenStreetMap and Overture. No web map check |
| Shiv Mandir | 247, 8 | 30 | overture | likely | Overture only. No web map check |

## Just outside the squares

These are real, or believed real, but their circles cannot reach the square, so they are not in the
files. Check them again if a district's centre moves. Distances are from the map and are rough.

| District | Site and how far past the edge |
|---|---|
| Karol Bagh | Carmel Prayer Mount Ministries 30 m east; Maariamman (South Indian) Temple 55 m east; Sarbojanin Shri Shyama Kali 80 m north; Gurdwara Sri Guru Singh Sabha 115 m east; Karol Bagh Gurudwara, Baba Poola Singh Marg, 135 m south |
| Laxmi Nagar | A Shiv temple 45 m north; Sheetla Mata Mandir 55 m south; a gurdwara 75 m south-east; Omkareshvar Mahadev Mandir 80 m east; Baba Balak Nath Mandir 95 m south |
| Saket | Khirki Masjid and Jama Masjid Saket, each about 25 m past the edge at the wall; Sankat Mochan Hanuman Mandir 25 m north; Shiv Temple, Sector 1, 90 m south |
| Ghaziabad | Shri Shani Dev Mandir 27 m west; Shrinivas Temple 30 m south; a Shiv Temple 35 m north; Deep Shaikel Varkas 65 m west; Sri Banke Bihari Shiv Mandir 80 m west; Shiv Mandir 110 m south |
| Indirapuram | SPS Residency Temple 55 m east; Shri Ram Janaki Mandir 85 m south |
| Sector 18 | Atta Peer 120 m north; Peer Baba Ki Mazaar 190 m north |
| Pari Chowk | Hazrat Bilal Masjid 30 m north-west |
| MG Road | Hare Krishna Mandir Gurugram 36 m north; a temple on Veer Savarkar Park Road 60 m south |
| Expressway | Delhi Bible Fellowship, A-07, 30 m west |
| Anand Vihar | Shiv Mandir, D-Block, 90 m north |
| Cyber City | Shri Shiv Shakthi Mandir 90 m south-east |
| Golf Course Road | Holy Name of Jesus Church 115 m east |
| Nehru Place | Shani Mandir and Shri Mandir, East of Kailash, about 140 m north. Kalkaji Mandir, ISKCON and the Lotus Temple are further off |
| Aerocity | ISKCON Mahipalpur about 235 m east |

## Looked at and left out

**Famous sites pinned in the wrong place** (Overture, from user pages): "Bangla Sahib Gurdawara" and
"Gurdwara Sis Gang Sahib" in Karol Bagh, "Gurudwara Rakab Ganj" on MG Road, "Akshardam" and "Sammed
Shikharji Madhuban" in Laxmi Nagar, "Jai Maa Kalka" by Nehru Place, and "Bishnoi Mandir Gurugram" on
Golf Course Road, which the web map has about 6 km away.

**Offices and shops that only sound holy:** Evangelical Fellowship of India, Shriharivirdhaashram and
Tree Of Life Mission (rooms in Nehru Place towers), Brahmawati Complex (a commercial block), The New
Body Temple (a gym), Temple Homes, Trip To Temples, Temple of Lightt, Dharmik App HQ, Ved Mandir
Trust. Christ's Commission Fellowship in MGF Metropolis Mall has one review, and a web search found no
such church in Gurugram.

**Pins that only match a name:** the "Gurudwara road" pin in Karol Bagh is a car park, and Guru
Silencers beside it is a shop. The pins along Holy Name Church Road off Golf Course Road are
businesses at that address. Faith Warriors Fellowship, "Pillar no-116", is far off in Noida.

**Web map junk.** Google's "place of worship" type holds personal names and nonsense, which appear to
be people's own listings: Krishna Thapa, Bipin sharma, Parwejalam, Car park, pillar 116, FaizAii, Raju,
Arista Sure, ShoaibMalik, Badarpur border, Panabajal, 7 Star Safe, S5, Cyber Hub SOCIAL, Delhi,
Laxminagar, shakarpur, Arbaj and others. A junk name
can sit on a real small shrine, so if a local knows better, add it. FaizAii, at the Anand Vihar bus
stand, is the likeliest of these.

**Leads that did not hold:** "Sanatan Dharam Mandir" in a Laxmi Nagar address (the real one is about
870 m north). Shri Hanuman Mandir at 9A/32 and Krishna Mandir in block 5A, Karol Bagh (those blocks
are west of the square). Masjid Kaptan Wali and Masjid Nai Walan from the Waqf list (by their streets
they are east of the square; not measured).

**Open leads with no position, so not in the files:**

- Laxmi Nagar: a Jain Mandir (businesses cite "Subhash Chowk" and "Jain Mandir Gali"), a Shani Mandir
  on Pusta Road, a "Gurudwara Wali Gali" in B block, and Bhakti Bharat's Neelam Mata Mandir, Shri
  Subha Sidhi Vinayak Mandir and Shanker Vihar Prachin Hanuman Mandir.
- Karol Bagh: Bhakti Bharat's Karol Bagh Kali Mandir and Guru Ravidas Vishram Dham.
- Golf Course Road: Vrindavan Temple at C-153, and a temple at B-4.
- Nehru Place: Bhakti Bharat says a Shiv Temple is near the metro, and gives no address.

## Doubts and limits

- **Nobody has walked these squares.** Small roadside shrines and temples inside housing societies
  that no listing shows are missing for certain. Karol Bagh and Laxmi Nagar will have more.
- **Mosques are the weakest part.** Only one mosque is listed in all 15 squares, and it is a doubt.
  A mosque search on every square showed no other pin inside one: the nearest are in Naiwala,
  Ramesh Park and Ghaziabad's old town. That is what the sources say, but it is surprising for Karol
  Bagh and Laxmi Nagar, and small neighbourhood mosques are the kind of site maps miss. Each search
  shows only about ten results. The UP and Haryana Waqf registers and the full Delhi register were
  not checked.
- No cremation ground, and no cemetery beyond Saket's, turned up in any square. The nearest
  cremation grounds found (Hindon Ghat, Sector 94, Geeta Colony, Sector 26 Gurugram) are all well
  outside.
- **Web map pins were read off screenshots** at about 1.4 m a pixel, so about 15 m out, and the view
  shows only what its top results and labels show.
- **Floor loot has no test on the NCR map yet.** Chests were checked on the built map on four seeds.
  Floor loot goes through the same filter in `delhi.rs`, but nothing checks the result.
- **Zone rings have little room in the busiest districts.** The 120 m rule now closes 99 % of
  Laxmi Nagar's kept ground to ring centres, 92 % of Karol Bagh's, 89 % of Pari Chowk's and 76 % of
  Saket's. That is the rule working, but a final circle will rarely end in those places.
- Holy names remain in the `pois` field of `ncr_laxminagar.json` and `ncr_saket.json`, and in some
  building names. `compose.rs` does not copy district `pois` today, so they do not reach the screen.

## How it was checked

On 1 Nov 2026, on the built NCR map:

- Seeds 1, 7, 23 and 99: 0 of 144 chests inside any circle, read from `ncr_atlas`. The same check
  on a build with the old lists finds 4 to 7 chests inside these circles, so it can fail. The number
  of chests did not drop.
- `cargo test -p shared --lib`: 29 pass, including `maps_are_unchanged` and the zone ring test.
- Comparing all 17 files with copies from before: only `holy` changed, byte for byte, and no old
  circle inside a square was lost.
- The real game was not run for this.

## Adding or changing a site

Edit the district's rows in `HOLY` in `tools/osm/ncr.py`, then run:

    python3 tools/osm/ncr.py holy [area ...]

That changes only the `holy` field. It also names anything OpenStreetMap marks as holy inside a
square that no circle covers, which catches new sites in fresh raw data. The JSON is built into the
game, so rebuild to see the change. Do not copy a position from Google Maps or Wikimapia: find the
site, then choose a point on an OpenStreetMap building or road beside it.
