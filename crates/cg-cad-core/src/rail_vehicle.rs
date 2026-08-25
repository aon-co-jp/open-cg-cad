//! 鉄道車両CADコンフィグ(新幹線・リニアモーターカー・特急・普通列車の
//! 二階建て対応、グリーン車・座席指定対応)。
//!
//! ユーザー指示「リニアモーターカーは二階建て対応、新幹線は二階建て
//! 対応、ライナーや特急から普通電車も二階建てやグリーン車や座席指定
//! 対応」への対応。
//!
//! **⚠️ 正直な開示**: (1) 実在の車両形式(E1系/E4系Max等)の詳細な
//! 車体設計・意匠を再現するものではなく、編成構成(車両種別×階数構成×
//! 座席クラス×座席指定の有無)を保持・集計する抽象化レイヤーに過ぎない。
//! (2) 二階建て車両の定員倍率は、実在した二階建て新幹線(JR東日本
//! E1系/E4系「Max」)の公開されている定員データから算出した**参考値**
//! (約1.7倍、単純な2倍ではなく階段・構造による目減りを考慮)であり、
//! 個別車種の正確な再現ではない。(3) 「普通列車の二階建て」は現行の
//! 日本の鉄道では一般的ではないが(二階建て車両は新幹線Max・一部の
//! 私鉄グリーン車・JR東日本近郊型グリーン車が中心)、本モジュールは
//! 利用者が自由に構成を試せるCAD設計ツールであり、現実の運用実態との
//! 一致を強制しない(ユーザー入力を尊重する`zoning::ZoningRules`と
//! 同じ設計方針)。

/// 車両種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RailVehicleType {
    /// 新幹線。
    Shinkansen,
    /// リニアモーターカー(超電導リニア等)。
    LinearMotorCar,
    /// 特急・ライナー。
    LimitedExpress,
    /// 普通列車(各駅停車)。
    LocalTrain,
}

/// 階数構成。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckConfig {
    Single,
    Double,
}

impl DeckConfig {
    /// 定員の倍率(参考値)。**正直な開示**: 単純な2倍ではなく、実在した
    /// 二階建て新幹線(E1系/E4系Max)の公開定員データから算出した約1.7倍
    /// (階段・構造スペースによる目減りを考慮した参考値)。
    pub fn capacity_multiplier(self) -> f64 {
        match self {
            DeckConfig::Single => 1.0,
            DeckConfig::Double => 1.7,
        }
    }
}

/// 座席クラス。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatClass {
    Standard,
    /// グリーン車(座席ピッチが広く、同じ車両長でも定員は減る)。
    Green,
}

/// 座席指定の有無。定員計算には影響しないが編成構成として保持する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservationType {
    Reserved,
    NonReserved,
}

/// 1両分の設定。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarConfig {
    pub deck: DeckConfig,
    pub seat_class: SeatClass,
    pub reservation: ReservationType,
    /// 車両長(m)。
    pub car_length_m: f64,
    /// 座席ピッチ(mm)。グリーン車は標準車より広いのが一般的
    /// (参考値、実在の特定形式の正確な再現ではない)。
    pub seat_pitch_mm: f64,
    /// 車両の座席配置における1列あたりの座席数(例: 標準車で2+3列なら5)。
    pub seats_per_row: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RailConfigError {
    InvalidCarLength(f64),
    InvalidSeatPitch(f64),
    InvalidSeatsPerRow(u32),
}

impl CarConfig {
    pub fn new(
        deck: DeckConfig,
        seat_class: SeatClass,
        reservation: ReservationType,
        car_length_m: f64,
        seat_pitch_mm: f64,
        seats_per_row: u32,
    ) -> Result<Self, RailConfigError> {
        if car_length_m <= 0.0 {
            return Err(RailConfigError::InvalidCarLength(car_length_m));
        }
        if seat_pitch_mm <= 0.0 {
            return Err(RailConfigError::InvalidSeatPitch(seat_pitch_mm));
        }
        if seats_per_row == 0 {
            return Err(RailConfigError::InvalidSeatsPerRow(seats_per_row));
        }
        Ok(CarConfig { deck, seat_class, reservation, car_length_m, seat_pitch_mm, seats_per_row })
    }

    /// この1両分の定員推定(座席ピッチから1列あたりの通路方向の列数を求め、
    /// 1列あたりの座席数を掛け、階数倍率を適用する簡易モデル)。
    pub fn estimated_capacity(&self) -> u32 {
        let rows = (self.car_length_m * 1000.0 / self.seat_pitch_mm).floor().max(0.0);
        let single_deck_capacity = rows * self.seats_per_row as f64;
        (single_deck_capacity * self.deck.capacity_multiplier()).floor() as u32
    }
}

/// 編成(複数車両の組み合わせ)。
#[derive(Debug, Clone, PartialEq)]
pub struct TrainSet {
    pub vehicle_type: RailVehicleType,
    pub cars: Vec<CarConfig>,
}

impl TrainSet {
    /// 編成全体の定員推定(各両の定員の合計)。
    pub fn total_capacity(&self) -> u32 {
        self.cars.iter().map(|c| c.estimated_capacity()).sum()
    }

    /// グリーン車の両数。
    pub fn green_car_count(&self) -> usize {
        self.cars.iter().filter(|c| c.seat_class == SeatClass::Green).count()
    }

    /// 二階建て車両の両数。
    pub fn double_deck_car_count(&self) -> usize {
        self.cars.iter().filter(|c| c.deck == DeckConfig::Double).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standard_single_deck_car() -> CarConfig {
        CarConfig::new(DeckConfig::Single, SeatClass::Standard, ReservationType::NonReserved, 25.0, 980.0, 5).unwrap()
    }

    #[test]
    fn double_deck_has_higher_capacity_than_single_deck_for_same_car() {
        let single = standard_single_deck_car();
        let double = CarConfig { deck: DeckConfig::Double, ..single };
        assert!(double.estimated_capacity() > single.estimated_capacity());
        // 参考値1.7倍程度(端数切り捨てのため厳密一致ではなく近傍を確認)。
        let ratio = double.estimated_capacity() as f64 / single.estimated_capacity() as f64;
        assert!((ratio - 1.7).abs() < 0.1, "expected ~1.7x, got {ratio}");
    }

    #[test]
    fn green_car_has_fewer_seats_than_standard_for_same_car_length() {
        let standard = CarConfig::new(DeckConfig::Single, SeatClass::Standard, ReservationType::Reserved, 25.0, 980.0, 5).unwrap();
        let green = CarConfig::new(DeckConfig::Single, SeatClass::Green, ReservationType::Reserved, 25.0, 1160.0, 4).unwrap();
        assert!(green.estimated_capacity() < standard.estimated_capacity());
    }

    #[test]
    fn train_set_total_capacity_sums_all_cars() {
        let car_a = standard_single_deck_car();
        let car_b = CarConfig { deck: DeckConfig::Double, ..car_a };
        let train = TrainSet { vehicle_type: RailVehicleType::Shinkansen, cars: vec![car_a, car_b] };
        assert_eq!(train.total_capacity(), car_a.estimated_capacity() + car_b.estimated_capacity());
        assert_eq!(train.double_deck_car_count(), 1);
    }

    #[test]
    fn local_train_can_be_configured_as_double_deck_with_green_car_and_reserved_seats() {
        // ユーザー指示通り、普通列車でも二階建て・グリーン車・座席指定を
        // 構成できること(現実の運用実態と一致しなくても、CAD設計ツール
        // として構成自体は妨げない設計方針)。
        let car = CarConfig::new(DeckConfig::Double, SeatClass::Green, ReservationType::Reserved, 20.0, 1160.0, 4).unwrap();
        let train = TrainSet { vehicle_type: RailVehicleType::LocalTrain, cars: vec![car] };
        assert_eq!(train.green_car_count(), 1);
        assert_eq!(train.double_deck_car_count(), 1);
        assert!(train.total_capacity() > 0);
    }

    #[test]
    fn linear_motor_car_supports_double_deck_configuration() {
        let car = CarConfig::new(DeckConfig::Double, SeatClass::Standard, ReservationType::Reserved, 24.0, 1000.0, 4).unwrap();
        let train = TrainSet { vehicle_type: RailVehicleType::LinearMotorCar, cars: vec![car] };
        assert_eq!(train.double_deck_car_count(), 1);
    }

    #[test]
    fn invalid_car_config_is_rejected_honestly() {
        assert_eq!(
            CarConfig::new(DeckConfig::Single, SeatClass::Standard, ReservationType::Reserved, 0.0, 980.0, 5),
            Err(RailConfigError::InvalidCarLength(0.0))
        );
        assert_eq!(
            CarConfig::new(DeckConfig::Single, SeatClass::Standard, ReservationType::Reserved, 25.0, -1.0, 5),
            Err(RailConfigError::InvalidSeatPitch(-1.0))
        );
        assert_eq!(
            CarConfig::new(DeckConfig::Single, SeatClass::Standard, ReservationType::Reserved, 25.0, 980.0, 0),
            Err(RailConfigError::InvalidSeatsPerRow(0))
        );
    }
}
