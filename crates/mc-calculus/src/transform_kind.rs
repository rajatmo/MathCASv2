//! The integral transforms by name (round 34: moved here from the transforms module, now `mc-transforms`, because
//! the derivative operator needs their names to tell a transform from an unknown function).

/// The integral transforms.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// `laplace_transform(f, t, s)`
    Laplace,
    /// `inverse_laplace_transform(F, s, t)`
    InverseLaplace,
    /// `bilateral_laplace_transform(f, t, s)`
    BilateralLaplace,
    /// `inverse_bilateral_laplace_transform(F, s, t)`
    InverseBilateralLaplace,
    /// `mellin_transform(f, x, s)`
    Mellin,
    /// `inverse_mellin_transform(F, s, x)`
    InverseMellin,
    /// `fourier_transform(f, t, w)`
    Fourier,
    /// `inverse_fourier_transform(F, w, t)`
    InverseFourier,
    /// `fourier_sin_transform(f, t, w)`
    FourierSin,
    /// `inverse_fourier_sin_transform(F, w, t)`
    InverseFourierSin,
    /// `fourier_cos_transform(f, t, w)`
    FourierCos,
    /// `inverse_fourier_cos_transform(F, w, t)`
    InverseFourierCos,
    /// `z_transform(f, n, z)`
    Z,
    /// `inverse_z_transform(F, z, n)`
    InverseZ,
}

impl Kind {
    /// The function's name.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Laplace => "laplace_transform",
            Kind::InverseLaplace => "inverse_laplace_transform",
            Kind::BilateralLaplace => "bilateral_laplace_transform",
            Kind::InverseBilateralLaplace => "inverse_bilateral_laplace_transform",
            Kind::Mellin => "mellin_transform",
            Kind::InverseMellin => "inverse_mellin_transform",
            Kind::Fourier => "fourier_transform",
            Kind::InverseFourier => "inverse_fourier_transform",
            Kind::FourierSin => "fourier_sin_transform",
            Kind::InverseFourierSin => "inverse_fourier_sin_transform",
            Kind::FourierCos => "fourier_cos_transform",
            Kind::InverseFourierCos => "inverse_fourier_cos_transform",
            Kind::Z => "z_transform",
            Kind::InverseZ => "inverse_z_transform",
        }
    }

    /// Every transform, for registration.
    pub const ALL: [Kind; 14] = [
        Kind::Laplace,
        Kind::InverseLaplace,
        Kind::BilateralLaplace,
        Kind::InverseBilateralLaplace,
        Kind::Mellin,
        Kind::InverseMellin,
        Kind::Fourier,
        Kind::InverseFourier,
        Kind::FourierSin,
        Kind::InverseFourierSin,
        Kind::FourierCos,
        Kind::InverseFourierCos,
        Kind::Z,
        Kind::InverseZ,
    ];

    /// Is the transform one of the Fourier kinds (with `fourier_parameters:`)?
    pub fn fourier_like(self) -> bool {
        matches!(
            self,
            Kind::Fourier | Kind::InverseFourier | Kind::FourierSin | Kind::InverseFourierSin | Kind::FourierCos | Kind::InverseFourierCos
        )
    }
}
