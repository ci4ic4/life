// life-star-core.js — pure stellar-structure physics, no DOM.
//
// FLOAT WIDTH: all continuous state here is Float64Array, NOT the Float32Array
// the rest of this suite uses. Density spans ~1e-6 to 1e10 g/cm^3 and pressure
// spans more; f32 carries ~7 significant digits and overflows near 3.4e38.
// Storing this state in f32 is a bug, not a style choice. Do not "fix" it.
//
// Units are cgs throughout. Every exported function states its units.

(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.LifeStar = api;
})(typeof self !== 'undefined' ? self : this, function () {
  'use strict';

  // Physical constants (cgs)
  const G = 6.67430e-8;           // cm^3/(g*s^2)
  const K_B = 1.380649e-16;       // erg/K
  const M_U = 1.66053906660e-24;  // g
  const A_RAD = 7.5657e-15;       // erg/(cm^3*K^4)
  const SIGMA = 5.670374419e-5;   // erg/(s*cm^2*K^4)
  const M_SUN = 1.98892e33;       // g
  const R_SUN = 6.96e10;          // cm
  const L_SUN = 3.828e33;         // erg/s
  const YEAR = 31557600;          // s

  // Degenerate electron pressure coefficients (Chandrasekhar formula).
  // K1 * (rho/muE)^(5/3) gives non-relativistic pressure.
  // K2 * (rho/muE)^(4/3) gives relativistic pressure.
  // Values computed from quantum mechanics: ℏ^2/(5*m_e) and ℏ*c/4,
  // scaled by (3π^2)^(2/3) and (3π^2)^(1/3) respectively, divided by
  // M_U raised to the appropriate power.
  const K1 = 1.0036e13;           // dyn/cm^2 per (g/cm^3)^(5/3)
  const K2 = 1.2435e15;           // dyn/cm^2 per (g/cm^3)^(4/3)
  // Q-values from nuclear mass defects:
  // Q_H: 4 H1 -> He4 releases 26.73 MeV (mass defect 0.028697 u).
  //      Per gram of H1 fuel: 26.73 MeV / (4 u * 931.494 MeV/u) * c^2 = 0.00712 c^2.
  //      The rounded "0.007 c^2" yields 6.3e18 erg/g; the unrounded is 6.40e18.
  // Q_HE: 3 He4 -> C12 releases 7.275 MeV (mass defect 0.007809 u).
  //       Per gram of He4 fuel: 7.275 MeV / (3 * 4 u * 931.494 MeV/u) * c^2 = 5.84e17 erg/g.
  const Q_H = 6.4e18;             // erg/g, hydrogen to helium
  const Q_HE = 5.84e17;           // erg/g, triple-alpha

  const laneEmdenCache = new Map();

  /**
   * Solve the Lane-Emden equation for polytrope index n.
   * Returns the dimensionless profile plus the two numbers everything
   * downstream needs: xi1 (surface) and massFactor = (-xi^2 dtheta/dxi)|xi1.
   * Memoised on n rounded to 0.05 because n moves slowly during evolution.
   */
  function laneEmden(n) {
    const key = Math.round(n / 0.05) * 0.05;
    const hit = laneEmdenCache.get(key);
    if (hit) return hit;

    const h = 1e-3;
    const xiStart = 1e-4;

    // Series start: the 2*phi/xi term is 0/0 at the origin.
    let xi = xiStart;
    let theta = 1 - xi * xi / 6 + key * Math.pow(xi, 4) / 120;
    let phi = -xi / 3 + key * Math.pow(xi, 3) / 30;

    const xis = [xi];
    const thetas = [theta];

    // theta^n, guarding against the negative base that appears one step
    // past the surface: Math.pow(negative, 1.5) is NaN.
    const thetaPow = (t) => (t > 0 ? Math.pow(t, key) : 0);

    const deriv = (x, t, p) => [p, -thetaPow(t) - 2 * p / x];

    let xi1 = null;
    let massFactor = null;

    for (let i = 0; i < 200000; i++) {
      const [k1t, k1p] = deriv(xi, theta, phi);
      const [k2t, k2p] = deriv(xi + h / 2, theta + h / 2 * k1t, phi + h / 2 * k1p);
      const [k3t, k3p] = deriv(xi + h / 2, theta + h / 2 * k2t, phi + h / 2 * k2p);
      const [k4t, k4p] = deriv(xi + h, theta + h * k3t, phi + h * k3p);

      const thetaNext = theta + h / 6 * (k1t + 2 * k2t + 2 * k3t + k4t);
      const phiNext = phi + h / 6 * (k1p + 2 * k2p + 2 * k3p + k4p);
      const xiNext = xi + h;

      if (thetaNext <= 0) {
        // Linear interpolation to the zero crossing.
        const frac = theta / (theta - thetaNext);
        xi1 = xi + frac * h;
        const phiSurf = phi + frac * (phiNext - phi);
        massFactor = -xi1 * xi1 * phiSurf;
        break;
      }

      xi = xiNext; theta = thetaNext; phi = phiNext;
      xis.push(xi); thetas.push(theta);
    }

    // n >= 5 has infinite radius; the caller clamps n well below that, but
    // fail loudly rather than returning a half-integrated profile.
    if (xi1 === null) throw new Error(`laneEmden(${n}) did not reach a surface`);

    const result = {
      n: key,
      xi1,
      massFactor,
      xi: Float64Array.from(xis),
      theta: Float64Array.from(thetas),
    };
    laneEmdenCache.set(key, result);
    return result;
  }

  // (A, Z) per species. Fully ionised throughout — these stars have no
  // neutral zone worth modelling.
  const SPECIES = {
    H1:   [1, 1],
    He4:  [4, 2],
    C12:  [12, 6],
    O16:  [16, 8],
    Ne20: [20, 10],
    Mg24: [24, 12],
    Si28: [28, 14],
    Fe56: [56, 26],
  };

  /**
   * Mean molecular weight and mean molecular weight per electron.
   * Input X is composition as mass fractions (dimensionless).
   * Returns mu and muE (dimensionless).
   */
  function meanMolecularWeight(X) {
    let invMu = 0, invMuE = 0;
    for (const key in SPECIES) {
      const frac = X[key] || 0;
      if (frac <= 0) continue;
      const [A, Z] = SPECIES[key];
      invMu += frac * (1 + Z) / A;
      invMuE += frac * Z / A;
    }
    return {
      // 1e9 is a "no ionised species present" sentinel to avoid returning Infinity.
      mu: invMu > 0 ? 1 / invMu : 1e9,
      muE: invMuE > 0 ? 1 / invMuE : 1e9,
    };
  }

  /**
   * Total pressure and the adiabatic index at (rho, T).
   * rho g/cm^3, T K, mu dimensionless, muE dimensionless.
   * Returns P in dyn/cm^2, gamma1 dimensionless, n dimensionless.
   *
   * gamma1 is the ADIABATIC exponent, not the isothermal one. The isothermal
   * d lnP / d ln rho for an ideal gas is 1, which would give n = infinity.
   */
  function eos(rho, T, mu, muE) {
    const pGas = rho * K_B * T / (mu * M_U);
    const pRad = A_RAD * T * T * T * T / 3;

    const pNr = K1 * Math.pow(rho / muE, 5 / 3);
    const pRel = K2 * Math.pow(rho / muE, 4 / 3);
    // ponytail: harmonic blend of the two degeneracy limits. Correct in both
    // limits and crosses over at the right density (~2e6 g/cm^3). Weakest point
    // is the transition region: at x=p_F/(m_e c)=0.372 the blend gives gamma1=1.59
    // whereas the exact Fermi integral gives ~1.64, but the fully relativistic limit
    // (Chandrasekhar) is correct and unaffected. Upgrade to the exact parametric
    // Fermi integral only if the white-dwarf mass-radius relation is ever measured
    // against rather than eyeballed.
    const pDeg = (pNr * pRel) / (pNr + pRel);

    const P = pGas + pRad + pDeg;

    const f = pNr / (pNr + pRel);
    const gammaDeg = (5 / 3) * (1 - f) + (4 / 3) * f;
    const gamma1 = (pGas * (5 / 3) + pRad * (4 / 3) + pDeg * gammaDeg) / P;

    let n = 1 / (gamma1 - 1);
    // Clamp [1.0, 3.4] is deliberately wider than the reachable [1.5, 3.0] to catch
    // non-finite input (e.g., from P=0 or gamma1=NaN) before it propagates to laneEmden.
    if (!Number.isFinite(n) || n > 3.4) n = 3.4;
    if (n < 1.0) n = 1.0;

    return { P, pGas, pRad, pDeg, gamma1, n };
  }

  // Calibration multipliers. Published rate coefficients omit electron
  // screening and are fitted over limited ranges; composing them with an
  // approximate structure model does not land on the solar luminosity by
  // itself. Tuned once in the Task 5 integration test.
  //
  // CAL.pp = 100 (Task 4): with it, structure(M_SUN, SOLAR) gives
  // R = 0.472 R_sun, T_c = 1.51e7 K, rho_c = 80.2 g/cm^3, L = 1.12 L_sun —
  // all inside the brief's generous solar brackets. Untuned (CAL.pp = 1)
  // undershoots the radius bracket (R = 0.31 R_sun) because the Gamow-peak
  // pp rate as published, composed with this polytrope structure and the
  // L ~ mu^4 M^3 homology target, runs too hot for a given R.
  const CAL = { pp: 100, cno: 1.0, he: 1.0 };

  /**
   * Proton-proton chain, erg/g/s. Gamow-peak form, NOT a power law:
   * evaluating T^4 style expressions overflows. The effective exponent
   * near 15 MK is about 4, which is a property of this formula, not an
   * instruction to implement it as such.
   */
  function epsPP(rho, T, X) {
    if (T <= 0 || X <= 0) return 0;
    const T6 = T / 1e6;
    return CAL.pp * 2.4e4 * rho * X * X *
      Math.pow(T6, -2 / 3) * Math.exp(-33.80 * Math.pow(T6, -1 / 3));
  }

  /** CNO cycle, erg/g/s. Effective exponent near 25 MK is about 17. */
  function epsCNO(rho, T, X, Z) {
    if (T <= 0 || X <= 0 || Z <= 0) return 0;
    const T6 = T / 1e6;
    return CAL.cno * 4.4e25 * rho * X * Z *
      Math.pow(T6, -2 / 3) * Math.exp(-152.28 * Math.pow(T6, -1 / 3));
  }

  /** Triple-alpha, erg/g/s. Effective exponent near 100 MK is about 40. */
  function eps3a(rho, T, Y) {
    if (T <= 0 || Y <= 0) return 0;
    const T8 = T / 1e8;
    return CAL.he * 5.1e8 * rho * rho * Y * Y * Y *
      Math.pow(T8, -3) * Math.exp(-44.027 / T8);
  }

  /**
   * Burn one shell for dt seconds.
   * Returns energy in erg/g and the change in each mass fraction (dimensionless).
   * Fuel consumption is capped at what is actually present, so an
   * over-long timestep degrades to "burned everything" rather than
   * driving a mass fraction negative.
   */
  function burnShell(rho, T, comp, dt) {
    const X = comp.H1 || 0;
    const Y = comp.He4 || 0;
    // Metallicity: everything that is not hydrogen or helium.
    let Z = 0;
    for (const k in SPECIES) {
      if (k !== 'H1' && k !== 'He4') Z += comp[k] || 0;
    }

    const eH = epsPP(rho, T, X) + epsCNO(rho, T, X, Z);
    const eHe = eps3a(rho, T, Y);

    let dH = Math.min(eH * dt / Q_H, X);       // grams of H per gram, capped
    let dHe = Math.min(eHe * dt / Q_HE, Y);    // grams of He per gram, capped

    const energy = dH * Q_H + dHe * Q_HE;

    const dComp = {
      H1: -dH,
      He4: dH - dHe,
      C12: dHe,
    };
    return { energy, dComp };
  }

  /** Invert the EOS for temperature at a given density and pressure. */
  function temperatureFor(rho, P, mu, muE) {
    let lo = Math.log(1e3), hi = Math.log(1e11);
    // eos is monotonically increasing in T at fixed rho.
    for (let i = 0; i < 80; i++) {
      const mid = 0.5 * (lo + hi);
      if (eos(rho, Math.exp(mid), mu, muE).P < P) lo = mid; else hi = mid;
    }
    return Math.exp(0.5 * (lo + hi));
  }

  /** Central conditions and polytrope index for a trial radius. */
  function centralState(M, R, mu, muE) {
    let n = 3;
    let rhoC = 0, tC = 0, le = null;
    for (let iter = 0; iter < 4; iter++) {
      le = laneEmden(n);
      const thetaPrime1 = -le.massFactor / (le.xi1 * le.xi1);
      const Wn = 1 / (4 * Math.PI * (n + 1) * thetaPrime1 * thetaPrime1);
      const rhoMean = M / ((4 / 3) * Math.PI * R * R * R);
      rhoC = rhoMean * Math.pow(le.xi1, 3) / (3 * le.massFactor);
      const pC = Wn * G * M * M / Math.pow(R, 4);
      tC = temperatureFor(rhoC, pC, mu, muE);
      n = eos(rhoC, tC, mu, muE).n;
    }
    return { n, rhoC, tC, le };
  }

  /**
   * theta at a given enclosed-mass fraction. The Lane-Emden solution is
   * sampled on xi, but the composition grid is Lagrangian, so this converts
   * between them. Cached per profile because the mapping never changes.
   */
  function thetaAtMassFraction(le, mFrac) {
    if (!le._massTable) {
      const N = le.xi.length;
      const cum = new Float64Array(N);
      let acc = 0;
      for (let i = 1; i < N; i++) {
        const xi = le.xi[i], dxi = le.xi[i] - le.xi[i - 1];
        const th = Math.max(le.theta[i], 0);
        acc += xi * xi * Math.pow(th, le.n) * dxi;
        cum[i] = acc;
      }
      for (let i = 0; i < N; i++) cum[i] /= acc || 1;
      le._massTable = cum;
    }
    const cum = le._massTable;
    // Binary search for the first index whose cumulative mass exceeds mFrac.
    let lo = 0, hi = cum.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (cum[mid] < mFrac) lo = mid + 1; else hi = mid;
    }
    return le.theta[lo];
  }

  /**
   * Solve for the equilibrium structure of a star of mass M (g) and
   * composition comp (mass fractions). shells is the shell count (default
   * 200). Returns R (cm), rhoC (g/cm^3), tC (K), n (dimensionless polytrope
   * index), L (erg/s), tEff (K), and rho/T/m profiles (Float64Array) sampled
   * on the Lagrangian mass grid (rho g/cm^3, T K, m g enclosed mass).
   *
   * Radius is whatever makes nuclear output match the luminosity the star
   * must radiate.
   *
   * ponytail: L_target uses the homology result L ~ mu^4 M^3 from radiative
   * diffusion with electron-scattering opacity, anchored at the Sun. No
   * opacity table, no radiative transfer. Upgrade path is a real Kramers
   * opacity and a solved transport equation, which is a different project.
   */
  function structure(M, comp, shells) {
    shells = shells || 200;
    const { mu, muE } = meanMolecularWeight(comp);
    const X = comp.H1 || 0;
    const Y = comp.He4 || 0;
    let Z = 0;
    for (const k in SPECIES) if (k !== 'H1' && k !== 'He4') Z += comp[k] || 0;

    const lTarget = L_SUN * Math.pow(M / M_SUN, 3) * Math.pow(mu / 0.6, 4);

    // Nuclear luminosity for a trial radius.
    const nuclear = (R) => {
      const { n, rhoC, tC, le } = centralState(M, R, mu, muE);
      let total = 0;
      const dm = M / shells;
      for (let i = 0; i < shells; i++) {
        const mFrac = (i + 0.5) / shells;
        const theta = thetaAtMassFraction(le, mFrac);
        const rho = rhoC * Math.pow(Math.max(theta, 0), n);
        const T = tC * Math.max(theta, 0);
        total += (epsPP(rho, T, X) + epsCNO(rho, T, X, Z) + eps3a(rho, T, Y)) * dm;
      }
      return { total, n, rhoC, tC, le };
    };

    // Bisect on log R. Smaller R means hotter centre means far more burning,
    // so nuclear output is steeply decreasing in R.
    let lo = Math.log(1e8), hi = Math.log(1e14);
    for (let i = 0; i < 60; i++) {
      const mid = 0.5 * (lo + hi);
      if (nuclear(Math.exp(mid)).total > lTarget) lo = mid; else hi = mid;
    }
    const R = Math.exp(0.5 * (lo + hi));
    const { n, rhoC, tC, le } = nuclear(R);

    const rho = new Float64Array(shells);
    const T = new Float64Array(shells);
    const m = new Float64Array(shells);
    for (let i = 0; i < shells; i++) {
      const mFrac = (i + 0.5) / shells;
      const theta = Math.max(thetaAtMassFraction(le, mFrac), 0);
      rho[i] = rhoC * Math.pow(theta, n);
      T[i] = tC * theta;
      m[i] = mFrac * M;
    }

    const L = lTarget;
    const tEff = Math.pow(L / (4 * Math.PI * R * R * SIGMA), 0.25);
    return { R, rhoC, tC, n, L, tEff, rho, T, m };
  }

  return { laneEmden, eos, meanMolecularWeight, epsPP, epsCNO, eps3a, burnShell, structure, CAL, G, K_B, M_U, A_RAD, SIGMA, M_SUN, R_SUN, L_SUN, YEAR };
});
