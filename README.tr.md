# Scholar Atıf Takip

<p align="center"><img src="docs/images/banner.png" alt="Scholar Atıf Takip" width="100%"></p>

<p align="center"><a href="https://github.com/bcankara/scholar-citation-tracker/releases/latest"><b>⬇ Windows veya macOS için indir</b></a> · <a href="README.md">English</a></p>

Herkese açık bir Google Scholar profilindeki atıf sayılarını zaman içinde izleyen küçük bir **Windows ve macOS** masaüstü uygulaması (Rust + Tauri 2). Kişisel kullanım için tasarlandı: kendi profilinizi girersiniz, uygulama sayfayı düşük sıklıkla kontrol eder ve geçmişi bilgisayarınızda saklar.

Arayüz **Türkçe** ve **İngilizce** olarak sunulur. İlk açılışta dil, işletim sisteminin görüntüleme diline göre seçilir; Ayarlar'dan istediğiniz zaman değiştirebilirsiniz.

## Öne çıkanlar

- Herkese açık Scholar profilinizi düşük sıklıkla okur; geçmişi (toplam, h-indeksi, i10-indeksi, makale bazlı sayılar) yerel bir SQLite veritabanında saklar.
- Son ziyaretinizden beri neyin değiştiğini tek cümleyle söyleyen bir panel.
- Scholar değişikliklerinin, geçmiş veriler açıksa atıf yapan yayınların kesin yayın tarihine göre takvimi ve tüm yılların ısı haritası.
- Yıl sonu tahmin aralığı ve hedefle birlikte kesintisiz eğilim çizgisi, en hızlı yükselen makaleler, zaman içinde h ve i10.
- Atıf yapanların ülke (dünya haritası), kurum, yayın yeri ve yayıncı, alan, yayın türü, açık erişim ve dile göre çözümlemesi; kimlerin atıf yaptığı (öz, ortak yazar, dış) ve etkili atıflar.
- Dergisi bilinen her atıf yapan yayının yanında 0-100 arası bir **OJP** dergi puanı (sıralanabilir; Analiz ve dışa aktarımlarda da yer alır), onu açıklayan bir Yardım sayfası ve bir Lisanslar ve emeği geçenler sayfası.
- Atıf yapan yayınların süzülebilir listesi (başlık DOI/OpenAlex kaydına bağlanır); her makale ve her Scholar değişikliğinden o makalenin Scholar'daki atıf listesine bağlantı.
- Seçilebilir grafiklerle PDF rapor, Excel dışa aktarma ve haftalık özet bildirimi.
- Türkçe ve İngilizce arayüz, açık ve koyu tema, uygulama içi imzalı güncellemeler.

## Tur

Tüm ekran görüntülerinde uydurma gösterim verisi vardır.

### Panel

Tek bir cümle, son ziyaretinizden beri neyin değiştiğini söyler. Altında dört toplam, güne göre gruplanmış son değişiklikler ve 12 aylık geçmiş yer alır.

<img src="docs/images/tr-panel.png" alt="Neyin değiştiğini söyleyen cümle, dört toplam, son değişiklikler ve 12 aylık geçmişle Panel" width="100%">

### Analiz: Takvim ve ısı haritası

Ay ızgarası, Scholar'ın değişiklik gösterdiği günleri ve geçmiş veriler açıksa atıf yapan yayınların yayımlandığı günleri işaretler. Bir günü seçince o günün makaleleri ve atıf yapan yayınları listelenir. Isı haritası tüm yılları birlikte gösterir.

<table>
<tr>
<td width="50%"><img src="docs/images/tr-calendar.png" alt="Kasım 2026'nın göründüğü, bir günün seçili olduğu ve Scholar değişikliklerinin listelendiği takvim kartı" width="100%"></td>
<td width="50%"><img src="docs/images/tr-heatmap.png" alt="Yıllara göre hangi günlerin dolu olduğunu gösteren, tüm yılları kapsayan ısı haritası" width="100%"></td>
</tr>
</table>

### Analiz: Eğilim

Tek kesintisiz birikimli çizgi, yıl sonuna aralık olarak çizilen tahmin, hedefiniz ve dönem başına atıf.

<img src="docs/images/tr-trend.png" alt="Kesintisiz çizgi, tahmin aralığı, hedef ve dönem başına atıfın göründüğü Analiz Eğilim sekmesi" width="100%">

### Analiz: Makaleler

En hızlı yükselen makaleler, makale başına Scholar ile tanımlı atıf yapan yayınlar arasındaki fark ve makaleye göre yığılmış aylık atıflar.

<img src="docs/images/tr-analysis-articles.png" alt="En hızlı yükselenler, Scholar ve tanımlı atıfların karşılaştırması ve yığılmış aylık grafikle Analiz Makaleler sekmesi" width="100%">

### Analiz: Endeksler

Zaman içinde h ve i10 basamak çizgileri ile sonraki basamağa en yakın makaleler.

<img src="docs/images/tr-indices.png" alt="Zaman içinde h ve i10 ile sonraki basamağa en yakın makalelerin göründüğü Analiz Endeksler sekmesi" width="100%">

### Analiz: Atıf yapanlar

Bu sekme "bana kim atıf yapıyor?" sorusunu her kategori için ayrı bir kartla yanıtlar. Dönem seçici, yayın süzgeci ve "Öz atıfları hariç tut" tüm sekmeye uygulanır; her grafik verinin kaç yayında eksik olduğunu yazar.

#### Ülkeler

Dünya haritası, ilk 10 ülke, yıllara göre ülkeler ve ilk kez atıf yapan ülkeler.

<img src="docs/images/tr-citers-countries.png" alt="Süzgeç çubuğu ile dünya haritası ve ilk 10 ülkenin göründüğü Atıf yapanlar sekmesi, Ülkeler kartı" width="100%">

#### Kurumlar

İlk 10 kurum; yeniler vurgulanır.

<img src="docs/images/tr-citers-institutions.png" alt="İlk 10 kurumu ve bu yıl yeni olanları gösteren Kurumlar kartı" width="100%">

#### Yayın yerleri ve yayıncılar

Atıf yapan yayınların dergileri ve yayıncıları.

<img src="docs/images/tr-citers-venues.png" alt="Yayın yerleri ve yayıncılar kartı" width="100%">

#### Dergi prestiji (OJP)

Atıf yapan yayınların dergilerinin OJP puanına göre dağılımı ve en çok atıf yapan dergiler, puanlarıyla.

<img src="docs/images/tr-citers-ojp.png" alt="Beyazdan kırmızıya on basamaklı histogram ve en çok atıf yapan 8 derginin OJP rozetleriyle tablosu olan Dergi prestiji kartı" width="100%">

#### Alanlar

Alanların treemap görünümü ve yayınlarınız ile alanlar arasında ısı haritası.

<img src="docs/images/tr-citers-fields.png" alt="Treemap ve yayın x alan ısı haritasıyla Alanlar kartı" width="100%">

#### Yayın türleri

Atıf yapan yayınların türü, açık erişim durumu ve dili.

<img src="docs/images/tr-citers-types.png" alt="Tür, açık erişim ve dil çubuklarıyla Yayın türleri kartı" width="100%">

#### Kimler atıf yapıyor

Öz atıfların (ORCID ve ad eşleşmesiyle), ortak yazar atıflarının ve dış atıfların payı ile en sık atıf yapan yazarlar.

<img src="docs/images/tr-citers-who.png" alt="Öz, ortak yazar ve dış atıfları ayıran, sık atıf yapan yazarları gösteren kart" width="100%">

#### Etkili atıflar

Etkili atıf yapan yayınların tablosu.

<img src="docs/images/tr-citers-influential.png" alt="Etkili atıflar tablosu" width="100%">

### Atıf yapanlar listesi

Bir grafikte çubuğa tıklayınca liste süzülür. Etkin süzgeçler, tek tek ya da hepsi birden kaldırılabilen çipler olarak görünür.

<img src="docs/images/tr-citing.png" alt="Ülke ve yayın yeri grafikleri, kaldırılabilir süzgeç çipleri ve 200 yayından 11'inin listelendiği Atıf yapanlar sayfası" width="100%">

OJP sütunu, her atıf yapan yayının dergisinin puanını rozet olarak gösterir; sıralamak için başlığına tıklayın (puanı olmayan yayınlar her zaman sonda gelir). Rozetin üzerine gelince alan, puan yoksa nedeni görünür.

<img src="docs/images/tr-citing-ojp.png" alt="Dergi OJP puanına göre sıralanmış, beyazdan kırmızıya rozetli Atıf yapanlar listesi" width="100%">

### Makaleler

Her makale; geçmiş grafiği ve sayısındaki her değişiklik, eşleşen OpenAlex kaydı (DOI düzeltme yoluyla) ve ona atıf yapan yayınlarla açılır.

<img src="docs/images/tr-articles.png" alt="İlk makalenin açık olduğu Makaleler sayfası: geçmiş grafiği, değişiklik geçmişi, OpenAlex eşleşmesi ve atıf yapan yayınlar" width="100%">

### Rapor (PDF) ve Excel

PDF için aralığı, dili ve grafikleri seçin; canlı önizleme bunları gösterir. Atıf yapanlar bölümleri (ülkeler ve kurumlar, alanlar, öz atıf) isteğe bağlıdır. Excel dışa aktarma Ayarlar'dadır.

<img src="docs/images/tr-report.png" alt="İsteğe bağlı Atıf yapanlar bölümleri işaretli ve canlı önizlemede görünen Rapor sayfası" width="100%">

### Ayarlar ve güncellemeler

Ayarlar; profil, aralık, dil, veri klasörü, API anahtarları, geçmiş veriler ve dışa aktarmayı içerir. Hakkında bölümü sürümü gösterir ve güncellemeleri denetler; sonuç her zaman gösterilir.

<img src="docs/images/tr-settings-about.png" alt="Uygulamanın güncel olduğunu gösteren, Hakkında bölümüne kaydırılmış Ayarlar" width="100%">

### Yardım ile Lisanslar ve emeği geçenler

**Yardım** sayfası OJP'yi (nedir, nasıl hesaplanır, ne değildir) açıklar ve diğer sayfalar için kısa notlar içerir. **Ayarlar > Hakkında > Lisanslar ve emeği geçenler** sayfası veri kaynaklarını lisanslarıyla, paketlenen bileşenleri ve uygulamaya derlenen her Rust kitaplığını, lisans metinleriyle birlikte listeler.

<img src="docs/images/tr-help.png" alt="Yardım sayfası: OJP nedir ve rozet nasıl okunur" width="100%">

<img src="docs/images/tr-credits.png" alt="Lisanslar ve emeği geçenler sayfası: uygulama, OpenAlex ve atıf bilgisiyle OJP veri seti" width="100%">

### Koyu tema

Arayüz, Windows'un açık/koyu ayarını izler.

<img src="docs/images/tr-dark-citers.png" alt="Dünya haritalı Ülkeler kartını gösteren koyu tema" width="100%">


## Ne yapar?

- Profil sayfasını belirli aralıklarla okur (varsayılan 3 saat; 1 ile 24 saat arasında ayarlanabilir) ve toplam atıf, h-indeksi, i10-indeksi, makale bazlı atıflar ile yıllık atıf dağılımını yerel bir SQLite veritabanına kaydeder.
- **Panel**, son ziyaretinizden beri ne değiştiğini tek cümleyle söyler; dört toplamı (h ve i10 kartları sonraki adımın ne istediğini de yazar, örneğin "h=9 için 3 atıf"), güne göre gruplanmış son değişiklikleri ve 12 aylık atıf geçmişini gösterir. Eğilimler, tahminler ve dökümler **Analiz** sayfasında, PDF **Rapor** sayfasındadır; bkz. [Panel, Analiz ve Rapor](#panel-analiz-ve-rapor). Makaleler sekmesinden her makalenin geçmişini, sayısındaki her değişiklikle (artış ya da azalış) birlikte açabilirsiniz.
- Atıf sayısı arttığında veya yeni bir makale göründüğünde sistem bildirimi gösterir (ilk kayıtta bildirim yoktur). Scholar bazen atıfları kaldırır (ve sonra geri ekleyebilir): azalışlar tablolarda ve değişim geçmişinde kırmızı gösterilir ama bildirim oluşturmaz. Aynı kontrolde hem artış hem azalış varsa bildirim net değişimi ve artan makaleleri gösterir.
- Başarısız denemeler 5 dakika arayla en fazla 3 kez daha tekrarlanır; her deneme "Kayıt günlüğü" sekmesinde görünür. Google isteği engellerse (HTTP 429 / 403, CAPTCHA sayfası) deneme "Engellendi" olarak kaydedilir ve uygulama bir sonraki döngüyü bekler.
- Pencereyi kapatınca uygulama Windows'ta bildirim alanına (tray), macOS'te menü çubuğuna saklanır. Bu menüde "Paneli aç", "Şimdi güncelle", "Veri klasörünü aç" ve "Çıkış" bulunur. Windows'ta simgeye sol tıklamak paneli açar, menü sağ düğmededir; macOS'te sol tıklamak menüyü açar, diğer bütün menü çubuğu simgelerinde olduğu gibi.
- İsteğe bağlı oturum açılışında başlatma (macOS'te Launch Agent olarak): uygulama gizli olarak, yalnızca tray'de ya da menü çubuğunda başlar. Varsayılan olarak açıktır, ancak bir profil girdikten sonra kaydedilir; Ayarlar'dan kapatılabilir.
- Özet geçmişi ve makale geçmişi CSV olarak ya da her şey tek bir Excel çalışma kitabı olarak dışa aktarılabilir (CSV UTF-8 BOM'ludur; Excel'de Türkçe karakterler doğru görünür). Sütun başlıkları seçili dile göre yazılır.
- Bir profil girmediğiniz sürece hiçbir şey çekilmez. İlk açılış kartı, profil kimliğini ya da profil adresinin tamamını (`https://scholar.google.com/citations?user=XXXX&hl=en`) ORCID kimliğinizle birlikte (zorunlu, bkz. [Geçmiş veriler](#geçmiş-veriler-isteğe-bağlı)) kabul eder.
- Profili sonradan değiştirirseniz, onayınızın ardından önceki profile ait tüm kayıtlar silinir.
- İsteğe bağlı [geçmiş veriler](#geçmiş-veriler-isteğe-bağlı): OpenAlex'ten, ORCID kimliğiniz üzerinden eşleştirilmiş gün, ay ve yıl düzeyinde geçmiş atıflar.
- Size [atıf yapan yayınların](#atıf-yapanlar-tahmin-raporlar-ve-dışa-aktarma) listesi, kesin tarihe göre atıf takvimi, tahmin ve hedef öngörüsü, en hızlı yükselen makaleler ve sonraki h / i10 adımı, seçilebilir grafiklerle PDF atıf raporu, Excel dışa aktarma ve haftalık özet bildirimi.
- Daha hızlı, istek sınırına daha az takılan taramalar için isteğe bağlı [API anahtarları](#api-anahtarları-isteğe-bağlı).

## Panel, Analiz ve Rapor

**Panel (ne değişti).** Profil adının altında tek bir cümle, *son ziyaretinizden beri* ne olduğunu söyler (pencerenin en son görüldüğü an, o andaki anlık görüntüyle karşılaştırılır). Öncelik: aşılan bir indeks eşiği ("h-indeksiniz 8'e çıktı"); yeni atıflar ve çoğunun geldiği makale; toplamda bir azalış (yansız bir dille); sonraki h'ye yakınlık. Hiçbir şey değişmediyse bunu ve son artışın ne zaman olduğunu söyler. Makale adları ve h ifadesi bağlantıdır. Altında: dört toplam (h ve i10 kartları sonraki adımın ne istediğini de yazar, örn. "h=9 için 3 atıf"), son değişiklikler (en çok altı satır, tarih başlıkları altında; bir tarih Takvim'i o günde açar) ve küçük bir 12 aylık geçmiş çizgisi.

**Analiz (eğilim, tahmin, takvim).** Bir dönem seçici (*Son 30 gün*, *Son 90 gün*, *Son 12 ay*, *Tümü*, *Özel*) grafiklere uygulanır; *Bu dönemi raporla* Rapor sayfasını o aralıkla açar. Gruplama (*Günlük / Aylık / Yıllık*) ayrı ve etiketli bir denetimdir. Dört sekme vardır; her grafiğin başlığı yanıtladığı sorudur:

- **Takvim** (varsayılan sekme): Windows CalendarView biçiminde bir ay ızgarası. Hareketli günler sayıya göre gölgelenir; mavi nokta **Scholar**'ın o gün gösterdiği değişikliği (gösterdiği güne göre), turkuaz nokta kesin yayın tarihli atıf yapan yayınları (**OpenAlex**), soluk kırmızı nokta yalnızca azalış olan günü işaretler. Bir güne tıklayın (aralık için ikinci güne Shift ile tıklayın ya da "Bu ayın tümü"); sağdaki panel o gün hangi makalelerinizin atıf kazandığını ya da kaybettiğini ("Scholar'da gör" ile) ve o gün hangi atıf yapan yayınların yayımlandığını listeler. Oklar, Home / End ve PageUp / PageDown ızgarada gezdirir, Enter seçer. Süzgeçler: kaynak (Hepsi / Scholar / OpenAlex) ve makale. Yalnızca ay ya da yıl bilgisi olan yayınlar hiçbir güne konmaz; ayrı bir "Tarihi belirsiz (N)" listesindedir. Ayın altında: yıl ısı haritası (tıklayınca o güne gider) ve birikimli geçmiş çizgisi.
- **Eğilim**: tek kesintisiz birikimli çizgi. İlk Scholar kontrolünden önceki geçmiş OpenAlex'ten (atıf yapan yayınlar, yayın tarihine göre) gelir ve son noktası ilk Scholar değerine eşit olacak biçimde orantılanır (boşluk ya da sıçrama yok; ince bir çizgi "Scholar takibi başladı" der). Sonrasında çizgi Scholar kontrollerini izler. Yıl sonuna tahmin mor renkte, gölgeli bir aralıkla sürer; kontroller 60 günü kapsayana kadar gizli kalır. Hız, anlık görüntüler arasındaki 30 günlük pencerelerden hesaplanır; bu yüzden ilk taramanın (Scholar'ın zaten saymış olduğu her şey) sıçraması hiçbir zaman büyüme sayılmaz. Altında: aralık olarak yıl sonu tahmini, aylık ortalama, son 12 ay, hedef ve dönem başına atıf (bir çubuğa tıklayınca tarihleri Takvim'de açılır).
- **Makaleler**: en hızlı yükselen makaleler, makale başına Scholar ile tanımlı atıf yapan yayınlar arasındaki fark ve makaleye göre yığılmış aylık atıflar (ilk beş ve "Diğer").
- **Endeksler**: kontroller boyunca h ve i10 basamak çizgileri ve sonraki h ile i10'a en yakın makalelerin tam listesi.

Her açıklama kullandığı tarihi söyler: *Scholar'ın değişikliği gösterdiği gün* (kontroller) ya da *atıf yapan yayının yayın tarihi* (OpenAlex). İkisi, belirtilmeden aynı eksende karıştırılmaz.

**Rapor (PDF).** Aralığı, dili ve hangi grafiklerin gireceğini seçin (eğilim, dönem başına, makale bazında, endeksler, takvim, ülke ve dergi). Küçük bir canlı önizleme aynı grafikleri gösterir. PDF, ekrandakiyle aynı grafik koduyla, vektör olarak çizilir; sayılar eşleşir.

### Scholar ile OpenAlex aynı sayıda saymaz ve OpenAlex geç kalır

Scholar daha çok ve daha erken sayar; OpenAlex yeni yayınları birkaç haftalık gecikmeyle ekler ve bazı yayınları hiç bilmez. Uygulama farkı saklamaz, gösterir:

- **Makale başına:** "Scholar 59 · tanımlı 40 · henüz tanımlanamayan 19" ve ince iki parçalı bir çubuk (Makaleler sekmesi ve Analiz > Makaleler).
- **En hızlı yükselenler** dayandığı veriyi söyler. Önce Scholar anlık görüntü artışları gelir. Anlık görüntü geçmişi pencereden kısaysa bunu yazar ("Scholar takibi 3 gündür: bu sürede +5") ve OpenAlex'e dayanan satırları "OpenAlex'e göre (atıf yapan yayının tarihi)" etiketiyle ayrı listeler. Her zaman "OpenAlex'teki en yeni atıf: <tarih>" der; gösterilecek bir şey yoksa nedenini (henüz Scholar geçmişi yok ve/veya o tarihten sonra OpenAlex kaydı yok) ve ne denenebileceğini yazar.
- **Gecikme kuyruğu** (OpenAlex'teki en yeni atıf yapan yayından sonraki günler) Takvim'de ve dönem çubuklarında "henüz indekslenmedi olabilir" ile hafifçe taranır; sıfır atıf demek değildir. OpenAlex'e dayanan hızlar, en yeni atıf yapan yayından sonraki ayları yok sayar.
- **Muhtemel eşleştirme.** Scholar'ın gösterdiği bir artış (makale, gün, +n), aynı makalenin, yayın tarihi olaydan 45 gün önce ile 15 gün sonra arasında olan atıf yapan yayınlarına, en yakın önce olmak üzere, açgözlü ve birebir bağlanır. Gün ayrıntısı bunları iç içe "muhtemelen: <başlık> · <dergi>" olarak gösterir; eşleşmeyen artışlar "Scholar'da görüldü — ayrıntı henüz yok" kalır. Azalışlar hiçbir zaman eşleştirilmez. Bağlantılar her seferinde verilerden türetilir (hiçbir şey saklanmaz); OpenAlex yetiştikçe kendiliğinden dolar. Bunlar tahmindir ve her zaman böyle etiketlenir.

## Geçmiş veriler (isteğe bağlı)

Scholar verisi yalnızca uygulamanın kurulduğu günden itibaren toplanır. İsterseniz uygulama geçmişi gün, ay ve yıl düzeyinde ücretsiz kaynaklardan doldurabilir. Bu özellik isteğe bağlıdır ve siz seçene kadar kapalıdır: ilk açılış kurulumundan ve onaylanmış bir profil değişikliğinden sonra bir pencere önerir ("Geçmişi de araştır" / "Bugünden başlat"); **Ayarlar > Geçmiş veriler** bölümünde anahtar, "Şimdi araştır" düğmesi, durum satırı (son çalışma, eşleşen makaleler, atıf kayıtları) ve "Geçmiş verileri sil" düğmesi bulunur.

- **ORCID kimliği (zorunlu).** İlk açılış kartı ve Ayarlar, Scholar profilinin yanında ORCID kimliğinizi ister. `0000-0002-1825-0097` ya da `https://orcid.org/0000-0002-1825-0097` girebilirsiniz; biçim ve ISO 7064 mod 11-2 denetim hanesi doğrulanır, iki alan da geçerli olana kadar "Başla" düğmesi etkinleşmez. Kayıtlı ayarlarda ORCID kimliği yoksa kurulum kartı görünür ve yalnızca onu ister; Scholar takibi bu sırada sürer, yalnızca geçmiş veri araştırması ORCID'e ihtiyaç duyar. ORCID kimliğini sonradan değiştirirseniz eşleşmeler ona bağlı olduğundan geçmiş veriler silinir.
- **Eşleştirme.** ORCID kimliği bir OpenAlex yazarına çözülür. Yazarın yayınları `author.orcid:<kimlik>` ve `authorships.author.id:<yazar kimliği>` süzgeçleriyle toplanır (yayın kimliğine göre birleşim). Her Scholar makalesi bu kümedeki en iyi adaya atanır: başlık benzerliği (kelime bazlı Sørensen–Dice) en az 0,80 ve yayın yılı Scholar'daki yıldan en fazla bir yıl farklı. Bir yayın en çok bir makaleye verilir; bir makale ile onun sonradan eklenen "Version 2" kaydı gibi yakın kopyalar önce benzerlik, sonra yıl ile ayrılır. Bu kümede bulunamayan makaleler, daha sıkı 0,85 eşiğiyle genel OpenAlex başlık aramasına düşer. Eşleşmenin nasıl yapıldığı (`orcid`, `title_search` ya da `manual_doi`; bkz. [Geçmiş eşleşmeyi düzeltme](#geçmiş-eşleşmeyi-düzeltme-doi)) saklanır; hiçbir yerde eşleşmeyen makaleler "eşleşmedi" olarak gösterilir.
- **Kaynak.** Her makale için o makaleye atıf yapan yayınlar OpenAlex'ten istenir. Makaleler sekmesinde açılan satır sayıyı, örneğin "OpenAlex 40", eşleşme yöntemini ve OpenAlex kaydına ve DOI'ye bağlantıları gösterir. Yalnızca OpenAlex kullanılır, çünkü Semantic Scholar ve OpenCitations gerçek makaleler üzerinde karşılaştırıldı ve OpenAlex'te zaten bulunmayan en fazla birkaç atıf yapan yayın ekledi; Semantic Scholar'ın sıkı istek sınırı ise taramaları birkaç kat yavaşlattı. Crossref atıf tarihi vermeyip yalnızca toplam verdiği için (atıf yapanlar listesi yalnızca üyelere açıktır) kullanılmaz.
- **Tarihleme kuralı.** Bir atıf, atıf yapan yayının yayın tarihine göre tarihlenir: kaynakta varsa tam gün, yoksa ay ya da yıl. Yalnızca yılı olan kayıtlar Yıl görünümünde yer alır, Gün / Ay görünümlerinde yer almaz (grafik kaç kayıt olduğunu belirtir).
- **Sayılar Scholar'dan farklıdır.** Kaynaklar Google Scholar'dan farklı yayınları dizinler; bu yüzden toplamlar tutmaz. Hiçbir şey uzlaştırılmaz ve her grafik kaynağını adıyla belirtir. Tek istisna Eğilim çizgisidir: ilk Scholar kontrolünden önceki geçmiş, ilk Scholar değerine orantılı ölçeklenir (şekil korunur; grafik bunu söyler). Dönem çubukları gün, ay ya da yıl düzeyinde atıf yapan yayınları (OpenAlex) kullanır; geçmiş veri yoksa yalnızca Scholar yıl dağılımı vardır.
- **Çalışma.** İlk Scholar kontrolünden sonra (veri varsa hemen) arka planda, aynı anda tek çalıştırma olarak çalışır; Scholar zamanlayıcısını engellemez ve etkilemez; hatalar ayarlar kartında görünür. Sonraki bir kontrolde yeni makale görülürse otomatik araştırılır. Yeniden çalıştırma kayıtları ekleyip çoğaltmak yerine değiştirir. Anahtarsız, OpenAlex yaklaşık saniyede bir (HTTP 429'da geri çekilmeyle) çağrılır; anahtarla yaklaşık 0,4 sn arayla. Uzun bir makale listesi yine de zaman alır; bu yüzden çalıştırma bir [tarama panelinde](#tarama-paneli) gösterilir ve durdurulabilir.
- **Gizlilik.** İsteklerde genel bir User-Agent (`scholar-citation-tracker/<sürüm> (+depo adresi)`) bulunur, e-posta adresi yoktur. Makale başlıkları, DOI'ler ve ORCID kimliğiniz OpenAlex'e gönderilir; bu özellik için Google'a hiçbir şey gönderilmez.

## Tarama paneli

Geçmiş veri taraması çalışırken sağ kenardan bir panel kayarak açılır (**Geçmiş veri taraması**; X ile kapatılır, üst çubuktaki "Geçmiş taranıyor 3/11" etiketinden ya da Ayarlar'dan yeniden açılır). Taramayı kendiniz başlattığınızda (ilk açılış seçimi, "Şimdi araştır", DOI düzeltmesi) kendiliğinden açılır; günlük otomatik yenileme yalnızca etiketi gösterir.

- Genel ilerleme çubuğu: "Makale 3 / 11", geçen süre ve tahmini kalan süre.
- Şu anki adım düz bir dille: "OpenAlex'ten atıf yapanlar alınıyor — sayfa 2, 87 kayıt" ya da bir sunucu uygulamadan yavaşlamasını istediğinde (HTTP 429 / 503) canlı geri sayımla "OpenAlex hız sınırı: 12 sn bekleniyor".
- Taramadaki her makale, durumu (sırada, çalışıyor, tamam, eşleşmedi, hata) ve sonucuyla, örneğin "OpenAlex 40".
- Son 30 olayı zamanlarıyla gösteren açılır **Ayrıntılar** günlüğü.
- **Durdur**, geçerli istekten sonra taramayı bitirir; o ana kadar kaydedilenler kalır, çalıştırma "durduruldu" olarak kaydedilir.
- Tarama bitince bir özet (eşleşen X / Y, kayıt sayısı, yeni olanlar, süre) siz paneli kapatana kadar kalır; eşleşmeyen ya da DOI'si olmayan makale sayısı da orada yer alır.

Dar pencerelerde panel tüm genişliği kaplar.

## Son atıflar ve Scholar'ın atıf listesi

Panelin **Son değişiklikler** kartı iki kaynağı tek bir listede, yeniden eskiye ve tarih başlıkları altında birleştirir (son altı kayıt; "Tümünü gör" daha fazlasını gösterir, "Takvimde aç" ve bir tarihe tıklamak Analiz > Takvim'i açar):

- **Scholar'da görülen değişimler.** Bir makalenin sayısı kayıtlı iki ardışık anlık görüntü arasında farklıysa "21 Kas — <makale> +2" gibi bir satır "Google Scholar" rozetiyle listelenir; azalış, "Scholar'da azaldı" etiketiyle kırmızı gösterilir. İlk anlık görüntünün öncesinde bir şey olmadığı için hiçbir zaman sayılmaz.
- Geçmiş araştırmanın (OpenAlex) bulduğu **atıf yapan yayınlar**; başlık, dergi, atıf yapılan makale ve kaynak rozetiyle.

Her Scholar satırında ve Makaleler sekmesindeki her makalede **Scholar'da gör** bulunur. Makalenin atıf listesini Scholar'da tarihe göre sıralı olarak **varsayılan tarayıcınızda** açar. Uygulama bu sayfayı kendisi okumaz: Scholar'ın arama sayfaları CAPTCHA ile korunur ve uygulama yalnızca herkese açık profil sayfanızı düşük sıklıkla okur. Bağlantı, profil sayfasının her makale için zaten içerdiği `cites=` kimliklerinden oluşturulur.

## Atıf yapanlar, tahmin, raporlar ve dışa aktarma

- **Atıf yapanlar (Atıf yapanlar sekmesi).** Geçmiş veriler açıkken, tarama atıf yapan her yayın için başlığını, yayın tarihini, dergisini (yayın yerini), DOI'sini, bağlantısını, yazarlarını, yazarların kurumlarının ülke kodlarını ve kayıt kimliğini saklar. Sekme hepsini listeler (tarih, DOI / OpenAlex sayfasına bağlı başlık, dergi, hangi makalelerinize atıf yaptığı); arama kutusu, makale süzgeci ve tarihe göre sıralama vardır. Makaleler sekmesinde bir makalenin açılan satırı da o makaleye atıf yapanları listeler. İki grafik, atıf yapan yayınların ilk 10 ülkesini ve ilk 10 dergisini gösterir (bir yayın, birden çok makalenize atıf yapsa da bir kez sayılır); bir alt yazı kaç yayında ülke bilgisinin olmadığını söyler. Atıf yapanların yıllara göre dağılımı zaten panelteki geçmiş grafiğinin içindedir. Ülke ya da dergi grafiğinde bir çubuğa tıklayınca liste o ülkeye ya da dergiye göre süzülür; etkin süzgeçler (ülke, dergi, Takvim'den tarih ya da dönem, makale) kaldırılabilir çipler olarak görünür.
- **Yeni atıf yapanlar.** Geçmiş veriler açıkken yaklaşık günde bir kez hafif bir yenileme çalışır (en erken açılıştan on dakika sonra, sonra her 24 saatte bir). Yalnızca eşleşmiş makalelerin atıf yapan listelerini, kayıtlı OpenAlex kimliğiyle, aşağıda anlatılan istek aralıklarıyla ve yeniden eşleştirme yapmadan okur. Daha önce kayıtlı olmayan atıf yapan yayınlar bulursa (ilk taramada asla) bir sistem bildirimi en fazla ikisini adıyla belirtir. "Bildirimler" ayarına uyar.
- **Tahmin (Analiz > Eğilim).** Yıl sonuna tahmin içeren tek kesintisiz birikimli çizgi; tahmin bir aralık olarak çizilir. Hız, Scholar anlık görüntüleri arasındaki 30 günlük pencerelerin ortalamasıdır (en yeniden başlayarak en çok altı), aralık pencerelerin ne kadar farklı olduğuna göre belirlenir, ilk tarama hiçbir zaman büyüme sayılmaz ve anlık görüntüler 60 günü kapsayana kadar tahmin gizlidir. Girdiğiniz hedef "<ay yıl> civarında N atıf" verir. Her zaman tahmin olarak etiketlenir; hedef yerel veritabanında saklanır.
- **En hızlı yükselenler (Analiz > Makaleler).** Son 30 ya da 90 günde en çok kazanan beş makale; dayandığı veri belirtilir (bkz. [Scholar ile OpenAlex aynı sayıda saymaz](#scholar-ile-openalex-aynı-sayıda-saymaz-ve-openalex-geç-kalır)). Sonraki h ve i10 için gerekenler Endeksler sekmesinde ve panel kartlarındadır.
- **PDF rapor (Rapor sekmesi).** Bir tarih aralığı, rapor dili (Türkçe ya da İngilizce) ve atıf yapanlar listesi ile grafiklerin eklenip eklenmeyeceği seçilir, ardından **PDF oluştur**. Sayfa numaralı A4 PDF; kapak sayfası (profil adı, ORCID kimliği, aralık, oluşturma tarihi), özet göstergeler (toplam, başlangıç yılından beri, h, i10, aralıktaki değişim), makale tablosu (aralık sonunda atıf, aralıkta kazanılan), grafikler (zaman içinde toplam, yıllara göre), isteğe bağlı olarak aralıktaki atıf yapanlar (tarih, başlık, dergi, atıf yapılan makale) ve kaynaklar-ve-yöntem notunu içerir. Rapor bir HTML sayfasıdır (`ui/report.html`); gizli bir pencerede çizilir ve platformun kendi webview yazdırmasıyla PDF'e yazdırılır (WebView2'de `PrintToPdf`, `WKWebView`'da bir `NSPrintOperation`), bu yüzden Türkçe karakterler sistem yazı tipinden gelir. İkisi de aynı A4 sayfaları üretir; macOS'te iş Core Printing ile doğrudan bir PDF dosyasına yönlendirilir, bu yüzden tanımlı bir yazıcı gerekmez.
- **Excel'e aktarma (Ayarlar > Dışa aktar).** Özet (anlık görüntü geçmişi), Makaleler (bugün / 7 / 30 günlük değişimlerle en son değerler), Makale geçmişi (uzun biçim), Atıf yapanlar ve Yıllık sayfalarını içeren tek bir .xlsx. Başlık satırları kalın ve renkli, sabitlenmiş ve süzülebilir; tarihler gerçek Excel tarihleridir; sayfa adları ve başlıklar arayüz diline göredir. İki CSV dışa aktarması da durur.
- **Haftalık özet.** Pazartesi günü saat 09:00'da ya da sonrasındaki ilk açılışta (yerel saat), ISO haftasında bir kez, "Bu hafta +N atıf; en çok: <makale> (+k)" bildirimi gösterilir. Haftada veri yoksa atlanır. Anahtar: **Ayarlar > Haftalık özet bildirimi** (varsayılan açık; "Bildirimler" de açık olmalı).

### Atıf yapanlar sekmesi (Analiz)

Analiz'in son sekmesi **Atıf yapanlar** "bana kim atıf yapıyor?" sorusunu kategori kategori, her biri kendi kartında yanıtlar: ülkeler (dünya haritası, ilk 10, yıllara göre ülke sayısı, ilk kez atıf yapan ülkeler), kurumlar (yeniler vurgulu), yayın yerleri ve yayıncılar, alanlar (treemap ve yayın x alan ısı haritası), yayın türleri (tür, açık erişim, dil), kimler (ORCID ve ad eşleşmesiyle öz atıf payı, ortak yazar ve dış atıflar, en çok atıf yapan yazarlar) ve etkili atıflar. Dönem seçici, yayın süzgeci ve "Öz atıfları hariç tut" tüm sekmeye uygulanır; her grafik verinin kaç yayında eksik olduğunu yazar. Veri, önceki OpenAlex isteklerinden gelir (ek `select` alanları); daha önce kaydedilen satırlar bir sonraki yenilemede tamamlanır. Her kartta küçük bir "listede gör" bağlantısı vardır; liste sayfasına tür, açık erişim, alan ve öz atıf süzgeçleri eklendi. PDF raporda üç isteğe bağlı bölüm (ülkeler ve kurumlar, alanlar, öz atıf), Excel sayfasında 13 yeni sütun vardır.

Analiz > Takvim'deki yıl ısı haritası artık tüm yılları aynı anda gösterir.

## OJP dergi puanı

**OJP** (Open Journal Prestige, Açık Dergi Prestiji), uygulamanın her atıf yapan yayının dergisinin yanında gösterdiği 0 ile 100 arasında bir sayıdır.

**Nedir**

- Bir derginin, kendi alanındaki dergiler arasındaki göreli konumudur: 0 en düşük, 100 en yüksek konumdur. OJP 76 ise dergi, alanındaki her 100 dergiden yaklaşık 76'sından daha yüksek puanlıdır.
- OpenAlex verisinden üretilmiş açık bir veri setindeki Article Network Score (ANS) değerinden hesaplanır: her alanda dergiler ANS'ye göre sıralanır ve sıra yüzdeliğe çevrilir (OJP = 100 x (daha düşük puanlı dergi sayısı + aynı puanlı dergi sayısının yarısı) / alandaki dergi sayısı, yuvarlanır). Her dergi tek bir alana aittir: makalelerinin en çok yer aldığı alan.
- Beyazdan sarıya, sarıdan kırmızıya koyulaşan bir rozetle gösterilir; bilgiyi sayı taşır, renk yalnızca sayıyı yineler. Sınıf ya da dilim yoktur.
- Göründüğü yerler: Atıf yapanlar listesi (sıralanabilir sütun), Analiz > Atıf yapanlar sekmesindeki bir kart (dağılım ve en çok atıf yapan dergiler), Excel dışa aktarımı (üç sütun ve "OJP hakkında" sayfası) ve isteğe bağlı olarak PDF rapor.
- Veri: 2024 sürümü (2024'te, 2019-2023 yıllarında yayımlanmış makale ve derlemelere yapılan atıflar), 25 alanda 85.948 dergi. Uygulamanın içinde gelir, hiçbir şey indirilmez, yalnızca yeni bir uygulama sürümüyle değişir. 1.4.0 öncesinde saklanan satırlar, sonraki yenileme OpenAlex kaynak kimliğini saklayana kadar ISSN ile eşleştirilir. Dosyaya ilişkin notlar: `src-tauri/data/ojp/README.md`.

**Ne değildir**

- Web of Science, Scopus, SJR ya da JCR göstergesi **değildir**, Q1-Q4 çeyrekliği de değildir. Bunlara dönüştürülemez ve onların yerine bildirilmemelidir.
- Türetildiği veri setinin resmî bir göstergesi değildir; veri setinin yazarı bu puanı incelememiş ve onaylamamıştır.
- Dergileri betimler; makaleleri ya da kişileri değil. Atama, yükseltme, ödül, proje desteği ve kişilerin benzeri resmî değerlendirmelerinde kullanılmamalıdır.
- Tek bir alan içindeki konumdur; farklı alanlardaki dergilerin puanları doğrudan karşılaştırılamaz.
- Dönemde sayılan makalesi 20'nin altında olan, kaynak veride puanı ya da alanı bulunmayan veya sıralanan dergi sayısı 20'nin altında kalan alanlardaki dergilere OJP verilmez ve **NA** ("puan şu an hesaplanmamıştır") görünür: dergi biliniyor ama sıralanmadıysa kesikli çerçeveli, veride yoksa düz NA; nedeni ipucunda yazar; dergi dışı yayınların (kitap, ön baskı, tez) hücresi boştur. Bazı yazılım ve veri dergileri eşiğin altında kalır.

Veri kaynağı ve lisansı [Lisanslar ve emeği geçenler](#lisanslar-ve-emeği-geçenler) bölümündedir; uygulamadaki Yardım sayfası aynı konuyu daha ayrıntılı anlatır.

## API anahtarları (isteğe bağlı)

**Ayarlar > API anahtarları (isteğe bağlı)** ücretsiz bir OpenAlex anahtarı girmenizi sağlar. Uygulama anahtarsız da çalışır.

- **OpenAlex.** Anahtar zorunlu değil. Anahtarsız kullanımda günlük bütçe IP adresine göre paylaşılır (üniversite ağlarında hızla tükenebilir) ve sınır saniyede 10 istektir; ücretsiz anahtar günlük bütçeyi 10 kat artırır ve sınırı saniyede 30 isteğe çıkarır. Anahtarı [openalex.org](https://openalex.org) adresinden alın (hesap açıp Ayarlar > API key bölümünden kopyalayın) ya da **E-postayla ücretsiz anahtar iste** düğmesine basın: e-posta adresinizi yazarsınız, uygulama `POST https://api.openalex.org/api-keys` çağrısını yapar; OpenAlex anahtarı (durum `pending_activation`) yanıtlar, uygulama onu saklar ve siz e-postanızdaki etkinleştirme bağlantısına tıklarsınız. Adres yalnızca o tıklamada gönderilir ve hiçbir yerde saklanmaz. [OpenAlex istek sınırları ve kimlik doğrulama](https://docs.openalex.org/how-to-use-the-api/rate-limits-and-authentication) sayfasına bakın. Anahtar URL'de değil, `Authorization: Bearer <anahtar>` üstbilgisiyle gönderilir.
- **Günlük bütçe doldu.** OpenAlex HTTP 429 ile `daily_budget` nedenini bildirirse uygulama taramayı temiz biçimde durdurur (kaydedilenler kalır, yarım okunmuş makale olmaz), çalıştırmayı "duraklatıldı" diye işaretler ve tarama panelinde, Ayarlar > API anahtarları kartında ve Panel, Analiz ile Atıf yapanlar sayfalarında kapatılabilir bir çubuk olarak bir bildirim gösterir; bildirimde bütçenin sıfırlanacağı saat (UTC gece yarısı, yerel saatinizle) ve ücretsiz anahtar, e-postayla anahtar isteme ve Ayarlar düğmeleri bulunur. Tarama, sıfırlamadan birkaç dakika sonra (ya da bir sonraki açılışta) kendiliğinden devam eder; o zamana kadar OpenAlex'e hiçbir istek gitmez. Saniye başına sınır yalnızca `Retry-After` kadar bekler. Sistem bildirimi günde en çok bir kez ve yalnızca sizin başlattığınız taramalar için gösterilir.
- **Saklama ve güvenlik.** Anahtarlar `settings.json`'a ya da veritabanına değil, platformun kimlik deposuna (Windows Kimlik Bilgileri Yöneticisi ya da macOS Anahtar Zinciri) `scholar-citation-tracker` hizmet adıyla yazılır. Günlüklere, çalıştırma iletilerine ve hata metinlerine asla girmez (anahtara benzeyen her şey maskelenir). **Sına** düğmesi ucuz bir istek yapar ve anahtarın geçerli olup olmadığını söyler. OpenAlex kayıtlı anahtarı reddederse (HTTP 401 / 403) uygulama bir kez uyarır ve anahtarsız devam eder.

## Veriler nerede?

- Veritabanı: Windows'ta `Belgelerim\ScholarTakip\scholar_takip.db`, macOS'te `~/Documents/ScholarTakip/scholar_takip.db` (SQLite, WAL kipi). Klasör Ayarlar'dan değiştirilebilir; yeni klasörde veritabanı yoksa mevcut olan oraya kopyalanır, varsa o kullanılır.
- Ayarlar: Windows'ta `%APPDATA%\io.github.bcankara.scholarcitationtracker\settings.json`, macOS'te `~/Library/Application Support/io.github.bcankara.scholarcitationtracker/settings.json`
- API anahtarları (girerseniz): Windows Kimlik Bilgileri Yöneticisi ya da macOS Anahtar Zinciri, hizmet adı `scholar-citation-tracker`.

## Geçmiş eşleşmeyi düzeltme (DOI)

Makaleler sekmesinde bir makaleyi açın ve geçmiş veri sütununa bakın. Yanlış ya da eksik eşleşmeyi üç eylem düzeltir:

- **DOI'yi düzelt.** Makalenin doğru DOI'sini `10.xxxx/...`, `doi:10.xxxx/...` ya da `https://doi.org/10.xxxx/...` biçiminde girin. Makale yalnızca bu DOI ile aranır (`https://api.openalex.org/works/doi:<doi>`); başlık eşleştirmesi yapılmaz, yöntem "elle DOI" olarak gösterilir. OpenAlex DOI'yi bilmiyorsa makale, DOI ile birlikte "eşleşmedi" olarak gösterilir.
- **Bu değil.** Şu anki eşleşmeyi bu makale için dışlar ve makaleyi yeniden arar; reddedilen OpenAlex kaydı bu makaleye bir daha önerilmez.
- **Otomatik eşleştirmeye dön.** Elle girilen DOI'yi ve reddedilen kayıtları siler, makaleyi yeniden otomatik eşleştirir.

Bunlardan birini kaydetmek, yalnızca o makale için aramayı hemen yeniden çalıştırır. Düzeltmeler yerel veritabanında saklanır; sonraki çalıştırmalardan ve "Geçmiş verileri sil" işleminden etkilenmez (hesaplanmış veri değil, sizin kararınızdır); yalnızca Scholar profilini değiştirdiğinizde, tüm kayıtlarla birlikte silinir.

## Makalelerin DOI'si

Makaleler sekmesi, DOI biliniyorsa (önce elle girilen DOI, yoksa eşleşen OpenAlex kaydının DOI'si) her başlığın altında bir **DOI** etiketi, bilinmiyorsa bir **DOI ekle** düğmesi gösterir. Düğme, satırı açmadan "DOI'yi düzelt" ile aynı pencereyi açar. Bazı makalelerin DOI'si eksikse, tablonun üstündeki bilgi çubuğu kaç tane olduğunu söyler ve listeyi onlara süzebilir (oturum boyunca kapatılabilir); panel ve tarama özeti bu süzülmüş listeye bağlantı verir. DOI, geçmiş verilerin ve atıf yapanların doğru kayda eşleşmesini sağlar.

## Sağlık uyarısı

Scholar 2 gün üst üste başarıyla okunamazsa (bu sürede deneme yapıldığı hâlde son başarılı okuma en az 48 saat önceyse) uygulama bir sistem bildirimi ("Scholar 2 gündür okunamıyor — Kayıt günlüğüne bakın") ve Panel'de bir sonraki başarılı okumaya kadar bir uyarı çubuğu gösterir. Bildirim olay başına bir kez, hiçbir zaman 24 saatte birden fazla gösterilmez ve "Bildirimler" ayarına uyar. Sayfa gelip de makale listesi okunamıyorsa da aynısı geçerlidir: profil atıf gösterip hiç makale göstermiyorsa ya da toplam atıf düşmediği hâlde makale sayısı yarıdan fazla azalırsa deneme Kayıt günlüğünde çözümleme hatası olarak kaydedilir ve anlık görüntü saklanmaz ("makale listesi okunamadı — Scholar sayfa yapısı değişmiş olabilir").

## Kurulum

### Windows

Kurulum dosyasını (`Scholar.Citation.Tracker_<sürüm>_x64-setup.exe`) **Releases** sayfasından indirip çalıştırın. Kurulum yalnızca geçerli kullanıcı için yapılır ve yönetici yetkisi gerektirmez. Kurulum sihirbazı Türkçe ve İngilizce sunulur. Uygulama Microsoft WebView2 çalışma zamanını gerektirir; güncel Windows 10 / 11 sürümlerinde zaten bulunur.

Kurulum dosyasını Dosya Gezgini'nde çift tıklayarak çalıştırın. Bir korumalı (sandbox) ya da paketlenmiş uygulamanın içinden (örneğin bir sohbet ya da mağaza uygulamasının dosya önizlemesinden) çalıştırmak Windows'un kurulumu sanallaştırmasına yol açabilir; bu durumda uygulama Yüklü uygulamalar listesinde görünmez.

**Kurulum dosyası kod imzalı değildir**; bu yüzden Windows SmartScreen ilk seferde *"Windows bilgisayarınızı korudu"* uyarısını gösterir. **Ek bilgi**'ye, ardından **Yine de çalıştır**'a tıklayın.

### macOS

Disk görüntüsünü (`Scholar.Citation.Tracker_<sürüm>_universal.dmg`) **Releases** sayfasından indirin, açın ve uygulamayı **Applications** klasörüne sürükleyin. Tek bir evrensel (universal) derleme hem Apple Silicon hem Intel Mac'leri kapsar; macOS 11 Big Sur veya üstü gerekir.

**Uygulama Apple Developer ID ile imzalı ve notarize edilmiş değildir**; bu yüzden Gatekeeper ilk açılışta reddeder (*"Apple ... içinde kötü amaçlı yazılım olmadığını doğrulayamadı"*). Ya Applications klasöründe uygulamaya sağ tıklayıp **Aç**'ı seçip onaylayın, ya da karantina işaretini bir kez kaldırın:

```
xattr -dr com.apple.quarantine "/Applications/Scholar Citation Tracker.app"
```

Uygulama menü çubuğunda bir simge tutar; pencereyi kapatmak uygulamayı kapatmaz, oraya saklar. Çıkmak için menü çubuğu simgesini ya da Cmd-Q'yu kullanın.

### İndirmeyi doğrulama

Uyarıların nedeni: kod imzalama sertifikası (macOS'te ayrıca Apple Developer Program üyeliği) her yıl ücretlidir ve bu, ikisi de olmayan küçük, kişisel bir açık kaynak projedir. Bunun yerine her iki derleme de bu depodaki kaynaktan GitHub Actions ile açıkça üretilir (bkz. `.github/workflows/release.yml`) ve indirdiğiniz dosyanın o iş akışının derlediği dosya olduğunu doğrulayabilirsiniz. Aşağıda `<dosya>` yerine indirdiğiniz dosyayı yazın.

1. **SHA-256.** Her sürümde Windows kurulum dosyasının özetini taşıyan bir `SHA256SUMS.txt` (önceki sürümlerdeki adla) ve 1.5.0'dan itibaren macOS disk görüntüsünün özetini taşıyan bir `SHA256SUMS-macos.txt` bulunur. İlgili olanı indirdiğiniz dosyanın özetiyle karşılaştırın:

   ```
   Get-FileHash .\<dosya> -Algorithm SHA256        # Windows, PowerShell
   shasum -a 256 <dosya>                           # macOS
   ```

2. **Derleme kaynağı kanıtı (provenance).** [GitHub CLI](https://cli.github.com/) kuruluyken:

   ```
   gh attestation verify <dosya> --repo bcankara/scholar-citation-tracker
   ```

   Bu, dosyanın tam olarak bu deponun GitHub Actions iş akışı tarafından derlendiğini doğrular.

### Güncellemeler

İlk kurulumdan sonra güncellemeler uygulamanın içinden kurulur. Açılıştan yaklaşık bir dakika sonra ve sonra günde bir kez (anahtar: **Ayarlar > Güncellemeleri otomatik denetle**) uygulama daha yeni bir sürüm arar. Varsa bir çubuk "Yeni sürüm X hazır" iletisini sürüm notlarıyla ve **Şimdi güncelle** düğmesiyle gösterir: güncellemeyi ilerleme göstergesiyle indirir, kurar ve uygulamayı yeniden başlatır. Windows'ta bu kurulum dosyasıdır; macOS'te uygulama paketi yerinde değiştirilir. **Ayarlar > Hakkında** sürümü, GitHub bağlantısını ve lisansı gösterir; sonucunu her zaman bildiren bir **Güncellemeleri denetle** düğmesi vardır (arka plandaki denetimin hataları sessizdir). Güncellemeler projenin güncelleme anahtarıyla imzalanır; anahtar uygulamanın içindedir ve herhangi bir şey kurulmadan önce denetlenir, bu yüzden bu yolda SmartScreen ya da Gatekeeper uyarısı yeniden görünmez.

## Kaldırma

**Windows.** **Ayarlar → Uygulamalar → Yüklü uygulamalar → Scholar Atıf Takip → Kaldır** yolunu izleyin ya da Başlat menüsündeki girdiye sağ tıklayıp **Kaldır**'ı seçin; uygulamadaki **Ayarlar → Hakkında → Kaldır…** düğmesi o Windows sayfasını açar. Kaldırıcı uygulama verilerinin silinmesini önerir. Atıf veritabanı, o klasörü silmediğiniz sürece (ya da Ayarlar'da veri klasörünü değiştirmediyseniz) `Documents\ScholarTakip` içinde kalır; ayarlar `%APPDATA%\io.github.bcankara.scholarcitationtracker` içindedir.

**macOS.** Önce Ayarlar'dan **Oturum açılışında çalıştır**'ı kapatın; böylece uygulama Launch Agent kaydını siler. Sonra menü çubuğu simgesinden çıkıp uygulamayı **Applications** klasöründen Çöp Kutusu'na taşıyın. Ev klasörünüzün dışına başka bir şey kurulmaz. Geride kalanlar: `~/Library/Application Support/io.github.bcankara.scholarcitationtracker` içindeki ayarlar ve `~/Documents/ScholarTakip` içindeki atıf veritabanı (ya da Ayarlar'da seçtiğiniz veri klasörü); tamamen silmek isterseniz ikisini de kaldırın.

## Kaynaktan derleme

**Windows** gereksinimleri:

- Rust (kararlı sürüm, MSVC araç zinciri)
- Visual Studio C++ Build Tools (C++ ile masaüstü geliştirme)
- Microsoft WebView2 çalışma zamanı
- Tauri CLI: `cargo install tauri-cli --version "^2"`

**macOS** (11 veya üstü) gereksinimleri:

- Rust (kararlı sürüm); evrensel derleme için ayrıca `rustup target add aarch64-apple-darwin x86_64-apple-darwin`
- Xcode Command Line Tools: `xcode-select --install`
- Tauri CLI: `cargo install tauri-cli --version "^2"`

Node.js ve npm gerekmez; arayüz `ui/` klasöründeki düz HTML/CSS/JS'tir.

### Kurulu uygulamanıza dokunmadan test

Gerçek bir kurulumla hiçbir şeyin çakışmaması için bir **geliştirme kimliği** derleyip çalıştırın (ayarlar, WebView verisi, tek-örnek kilidi ve otomatik başlatma girdisi farklıdır; kimlik deposu girdileri `SCT_KEYRING_SERVICE` ayarlıysa farklıdır, aşağıya bakın):

```
cargo tauri build --debug --no-bundle --config src-tauri/tauri.dev.conf.json
```

`src-tauri/tauri.dev.conf.json` `io.github.bcankara.scholarcitationtracker.dev` tanımlayıcısını ve "Scholar Citation Tracker Dev" ürün adını belirler. Ayarları `%APPDATA%\io.github.bcankara.scholarcitationtracker.dev` (macOS'te `~/Library/Application Support/io.github.bcankara.scholarcitationtracker.dev`), Windows'ta WebView verisi `%LOCALAPPDATA%\io.github.bcankara.scholarcitationtracker.dev` altında durur; tek-örnek kilidi `<tanımlayıcı>-sim`, otomatik başlatma değeri ise ürün adıyla adlandırılır. Bunun `settings.json` dosyasına açık bir geçici `data_dir` ile `"autostart": false, "auto_update": false` yazın, işiniz bitince `.dev` klasörlerini silin. Hata ayıklama derlemeleri ayrıca `SCT_KEYRING_SERVICE` (ayrı bir kimlik deposu hizmet adı) ve `SCT_TEST_PDF_PATH` (rapor PDF'ini sormadan oraya yazar) değişkenlerine uyar; sürüm derlemeleri ikisini de yok sayar.

`node tools/gen_licenses.js`, `ui/licenses/` klasörünü (uygulamaya derlenen Rust kitaplıkları ve lisansları; Lisanslar sayfası için) `src-tauri/Cargo.lock` dosyasından yeniden üretir; her bağımlılık değişikliğinden sonra çalıştırın. `node tools/gen_licenses.js --check` dosyalar güncel değilse başarısız olur (sürüm iş akışı bunu çalıştırır).

`node tools/check_i18n.js`, Türkçe ve İngilizce metinlerin eşleştiğini, arayüzün kullandığı her metnin bulunduğunu ve kısaltılmış dönem etiketlerinin (30G, 12A, 3Y) kalmadığını denetler.

`node tools/check_ojp_contrast.js`, OJP rozetinin karşıtlığını (sayı ile zemin, 10 basamak, açık ve koyu tema, rapor) CSS dosyalarından yeniden hesaplar ve 4,5:1 altında hata verir.

```
cd src-tauri
cargo test
cargo tauri build                                  # Windows ya da yalnızca derleyen Mac
cargo tauri build --target universal-apple-darwin  # macOS, iki mimari birlikte
```

Güncelleme çıktıları etkin olduğundan `cargo tauri build` bir imza anahtarı ister: `TAURI_SIGNING_PRIVATE_KEY_PATH` (`cargo tauri signer generate` ile üretilmiş anahtar dosyasının yolu) ve `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` ortam değişkenlerini ayarlayın; eşleşen açık anahtar `tauri.conf.json` içindeki `plugins.updater.pubkey` alanına yazılır. Kurulum dosyası ve `.sig` dosyası `src-tauri\target\release\bundle\nsis\` altında oluşur; macOS'te `.app`, `.dmg` ve imzalı `.app.tar.gz` dosyaları `src-tauri/target/universal-apple-darwin/release/bundle/` altına gelir.

Yalnızca tek bir platformu ilgilendiren ayarlar, Tauri CLI'nin kendiliğinden birleştirdiği ayrı bir yapılandırma dosyasında durur: `src-tauri/tauri.macos.conf.json` macOS paket hedeflerini ve en düşük sistem sürümünü içerir. `macOSPrivateApi` ayarlanmaz: Tauri 2.12.1'den beri bu ayar ve `macos-private-api` Cargo özelliği işlevsizdir (yarı saydam pencere onlarsız da çalışır); Cargo özellik listesi derlenen platformun birleşik yapılandırmasıyla uyuşmadığında tauri-build düz bir `cargo build`'i durdurur.

Arayüz simgelerini "Segoe Fluent Icons" yazı tipinin özel kullanım alanı kod noktalarıyla yazar; bu yazı tipi Windows ile gelir ve başka hiçbir yerde kurulu değildir. `ui/icons.js` yazı tipinin çizip çizemediğini ölçer; çizemediği yerde her kod noktasını aynı biçimi veren satır içi SVG ile değiştirir, SVG de yerini aldığı ögenin rengini ve boyutunu devralır, böylece mevcut boyut kuralları olduğu gibi çalışmayı sürdürür. Yalnızca kod noktasını tek başına taşıyan öge değiştirilir: bir simgeyi etiketiyle birlikte saran düğme, etiket henüz boşken de o tek karakter olarak okunur ve üzerine yazmak etiketi de götürür. Windows'ta dosya hiçbir şey yapmaz ve sistem yazı tipi eskisi gibi çizilir. Karşılığı olmayan bir kod noktasına dokunulmaz ve adı konsola bir kez yazılır, böylece sonradan eklenen bir simge sessizce boş kalmak yerine eksik biçim olarak görünür.

Canlı sayfanın hâlâ çözümlenebildiğini denemek için (tek istek) `src-tauri` klasöründe `cargo run -- --once <profil kimliği veya profil adresi>` komutunu çalıştırın; hata ayıklama derlemesi sonucu konsola yazar.

Testler, test kodunun içinde yer alan küçük, elle yazılmış bir profil sayfası kullanır. Gerçek bir profil sayfasının kayıtlı kopyasını `src-tauri/tests/fixtures/profile.html` yoluna koyarsanız ek tutarlılık kontrolleri de çalışır; dosya yoksa bu kontroller atlanır. `fixtures` klasörü git tarafından yok sayılır.

## Sürüm yayımlama (bakımcılar)

`src-tauri/tauri.conf.json` içindeki sürümle eşleşen bir `vX.Y.Z` etiketi gönderildiğinde GitHub Actions'ta `.github/workflows/release.yml` başlar. Testleri çalıştırır ve iki platformda paralel derler: `windows-latest` üzerinde NSIS kurulum dosyası, `macos-latest` üzerinde evrensel `.dmg`; ikisi de `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` gizli anahtarlarındaki güncelleme anahtarıyla imzalanır. Her iki indirmeyi, güncelleme dosyalarını ve `.sig` dosyalarını, `SHA256SUMS.txt` (Windows; önceki bütün sürümlerdeki ad) ile `SHA256SUMS-macos.txt` dosyalarını ve platform başına derleme kaynağı kanıtını içeren bir **taslak (draft)** sürüm oluşturur. Üçüncü bir iş, `latest.json` dosyasını iki derlemeden bir kez yazar; böylece güncelleme akışı `windows-x86_64`, `darwin-aarch64` ve `darwin-x86_64` anahtarlarını tek dosyada taşır. İçindeki adresler, sürüm yayımlandığında geçerli olacak genel indirme adresleridir (`https://github.com/<sahip>/<depo>/releases/download/<etiket>/<dosya>`, dosya adı GitHub'ın kaydettiği biçimde); bu yüzden `latest.json` artık elle düzeltilmez. Bu iş bir derleme başarısız olduğunda da çalışır: Windows zorunludur (Windows derlemesi bitmeden `latest.json` yazılmaz ve çalışma başarısız olur), macOS ise derlemesi bittiyse eklenir. macOS derlemesi başarısız olursa çalışma bir uyarı ve iş özetinde bir satır gösterir, `latest.json` yalnızca Windows'u kapsar; bu hâliyle yayımlamak Windows güncellemelerini çalışır tutar, Mac'ler ise kendi sürümlerinde kalır. Yalnızca Windows'u yayımlamak istemiyorsanız önce başarısız macOS işini yeniden çalıştırın (*Re-run failed jobs* bu işi de yeniden çalıştırır ve `latest.json`'ı değiştirir). Taslağı gözden geçirip yayımlayın. Uygulama içi güncelleyici `https://github.com/bcankara/scholar-citation-tracker/releases/latest/download/latest.json` adresini okur; bu adres yalnızca sürüm **yayımlandıktan** sonra vardır (taslak ona görünmez). İş akışının elle çalıştırılması (*Run workflow*) aynı dosyaları iş akışı çıktısı olarak üretir ve sürüm oluşturmaz.

## Sorumluluk reddi

Bu, resmî olmayan ve bağımsız bir projedir; Google ile bağlantılı değildir, Google tarafından desteklenmez veya onaylanmamıştır. Herkese açık profil sayfasını düşük bir istek sıklığıyla (varsayılan olarak 3 saatte bir) okur ve herhangi bir özel API kullanmaz. Google Scholar'ın resmî bir API'si yoktur; sayfa yapısı veya kullanım koşulları değişebilir ve bu durum uygulamayı her an bozabilir. Kullanımınızın Google'ın hizmet şartlarına uygun olmasından siz sorumlusunuz. Uygulama, kendi profilinizi izlemek için kişisel kullanıma yöneliktir; toplu veya yüksek sıklıklı veri çekme için kullanmayın.

## Lisanslar ve emeği geçenler

Aynı liste uygulamanın içinde de vardır (Ayarlar > Hakkında > Lisanslar ve emeği geçenler); her lisans metni oradan açılabilir.

### Veri kaynakları

- **OpenAlex** (https://openalex.org): atıf yapan yayınların künye bilgileri (dergi, yazar, kurum, ülke, alan). OpenAlex verisi CC0 ile kamuya açıktır.
- **Opindx** (OJP puanları), aşağıya bakın.
- **Natural Earth** (dünya haritası), aşağıya bakın.

### Kitaplıklar, simge ve yazı tipleri

Uygulama simgesi, Microsoft'un Fluent UI System Icons (MIT Lisansı) çalışmasını temel alır.

Grafikler [Apache ECharts](https://echarts.apache.org/) 5.5.1 (Apache Lisansı 2.0) ile çizilir; lisansı ve bildirimiyle birlikte `ui/vendor/` içinde paketlidir (`echarts.min.js`, SHA-256 `e84270bd0cd5bdf60fefc26d00c2a391cb2e81f4d26a7a9ee16185a54773a3cf`, npm kayıt defterindeki `echarts@5.5.1` paketinden); çevrimdışı çalışır, hiçbir şey CDN'den yüklenmez.

Dünya haritası: [Natural Earth](https://www.naturalearthdata.com/) 1:110m ülke sınırları (kamu malı), `nvkelso/natural-earth-vector` deposunun `v5.1.2` etiketi; `ui/vendor/world.geo.json` olarak sadeleştirildi (SHA-256 `5639a2568f0f14a602ba21da1310e9aebcc3dc6afb21417e4c96af7d64931e31`; özgün dosya `6866c877d39cba9c357620878839b336d569f8c662d3cfab4cb1dbe2d39c977f`); bildirim `ui/vendor/LICENSE-natural-earth.txt` içinde.

OJP verisi: Weitzel, U. (2026). *Opindx: Open Journal Factor, Network Factor and Article Network Score* (veri sürümü 2026-09-v5) [Veri seti]. Zenodo. https://doi.org/10.5281/zenodo.23599869; [Creative Commons Atıf 4.0 Uluslararası (CC BY 4.0)](https://creativecommons.org/licenses/by/4.0/) lisansıyla kullanılmıştır. Yapılan değişiklikler: 2024 atıf yılı dosyasından yalnızca OpenAlex evrenindeki Article Network Score (Raw) sütunu ile dergi kimlik sütunları alındı (süzme için makale sayısı ve OpenAlex evreni işareti de okundu; Norveç kaydına dayanan hiçbir sütun kullanılmadı); puanlar alan içi yüzdelik sıraya (OJP, 0-100) dönüştürüldü; sayılan makalesi 20'nin altında olan, puanı ya da alanı bulunmayan ve sıralanan dergi sayısı 20'nin altında kalan alanlardaki dergilere puan verilmedi. OJP resmî bir Opindx göstergesi değildir; veri setinin yazarı bu türetilmiş çalışmayı incelememiş ve onaylamamıştır. Opindx'in burada kullanılan sütunlarının sayısal girdisi OpenAlex verisidir (CC0, https://openalex.org). Paketlenen dosya `src-tauri/data/ojp/ojp-2024.csv.gz` (SHA-256 `034462a329917013c8f250e30762dc5b99ce88664bcd790b2193f36414c04e1b`; kaynak dosya `scores_2024.csv` SHA-256 `788de513f9d69f0418aa5798e755314b1fcba2312c2f419beac1531d8f0a1ad4`); atıf metni `src-tauri/data/ojp/NOTICE.txt` içindedir.

Uygulamaya derlenen Rust kitaplıklarının lisansları: `ui/licenses/rust-crates.json` ve metinleri `ui/licenses/rust-license-texts.json` (`tools/gen_licenses.js` ile üretilir; çoğu MIT ya da Apache-2.0, birkaçı MPL-2.0 ve Unicode-3.0). Yerel veritabanı SQLite'tır (kamu malı, `rusqlite` kitaplığı ile paketlenir). Uygulama yazı tipi içermez; Windows'ta yüklü Segoe UI ve Segoe Fluent Icons kullanılır. Uygulamanın kendisi [MIT lisansı](LICENSE) ile dağıtılır.

## Lisans

[MIT](LICENSE)
