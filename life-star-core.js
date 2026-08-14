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

  return { laneEmden, G, K_B, M_U, A_RAD, SIGMA, M_SUN, R_SUN, L_SUN, YEAR };
});
