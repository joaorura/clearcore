//! pDFNet3 Pro v2 (three stateful graphs `enc`, `erb_dec`, `df_dec` with `FiLM`) — the model
//! bundled today, used until the v3 FULL asset is installed.
//!
//! The ONNX Runtime engine mirrors `realtime_noise_model::ort_backend::OrtSpectralModel` (owned by
//! the daemon side, which this crate does not edit) so that it can run on this crate's own
//! [`realtime_noise_model::DspPipeline`] — the only way to apply the profile's spectral EQ, which
//! `OrtBackend::set_voice_profile` ignores. The OpenVINO engine wraps
//! [`realtime_noise_runtime_openvino::Dfn3Session`]. Both run the three graphs on every hop.

use deep_filter::Complex32;
use ort::session::Session;
use ort::value::Tensor;
use realtime_noise_model::dsp_pipeline::{DEFAULT_DF_ORDER, DEFAULT_NB_DF, DEFAULT_NB_ERB};
use realtime_noise_model::{ArchiveGraphs, ExtractedFeatures, FILM_HIDDEN_DIM, ModelFrameOutput};
use realtime_noise_runtime_openvino::{Dfn3Graphs, Dfn3Output, Dfn3Session};

use crate::engine::{Conditioning, Engine};
use crate::v3::{ort_fill, ort_read, ort_session, ort_zeros};

const EMB_DIM: i64 = 512;
const GRU_HIDDEN: i64 = 256;
const DF_COEFS_LEN: usize = DEFAULT_NB_DF * DEFAULT_DF_ORDER * 2;

fn declares_film(session: &Session) -> bool {
    let names: Vec<&str> = session.inputs().iter().map(|i| i.name()).collect();
    names.contains(&"gamma") && names.contains(&"beta")
}

fn to_prediction(lsnr: f32, mask: &[f32], coefs: &[f32]) -> Result<ModelFrameOutput, String> {
    if mask.len() != DEFAULT_NB_ERB || coefs.len() != DF_COEFS_LEN {
        return Err("unexpected mask/coefs size".to_owned());
    }
    if !lsnr.is_finite() || mask.iter().chain(coefs).any(|v| !v.is_finite()) {
        return Err("non-finite model output".to_owned());
    }
    let mut erb_gains = [0.0f32; DEFAULT_NB_ERB];
    erb_gains.copy_from_slice(mask);
    Ok(ModelFrameOutput {
        lsnr,
        erb_gains: Some(erb_gains),
        df_coefs: Some(
            coefs
                .chunks_exact(DEFAULT_DF_ORDER * 2)
                .map(|b| std::array::from_fn(|t| Complex32::new(b[2 * t], b[2 * t + 1])))
                .collect(),
        ),
    })
}

fn film_values(conditioning: &Conditioning) -> [Vec<f32>; 4] {
    match &conditioning.film {
        Some(f) => [
            f.gamma_enc.clone(),
            f.beta_enc.clone(),
            f.gamma_df.clone(),
            f.beta_df.clone(),
        ],
        None => [
            vec![1.0; FILM_HIDDEN_DIM],
            vec![0.0; FILM_HIDDEN_DIM],
            vec![1.0; FILM_HIDDEN_DIM],
            vec![0.0; FILM_HIDDEN_DIM],
        ],
    }
}

/// v2 graphs on ONNX Runtime (CPU).
pub struct V2OrtEngine {
    enc: Session,
    erb_dec: Session,
    df_dec: Session,
    feat_erb: Tensor<f32>,
    feat_spec: Tensor<f32>,
    enc_h: Tensor<f32>,
    enc_erb_buf: Tensor<f32>,
    enc_spec_buf: Tensor<f32>,
    erb_emb: Tensor<f32>,
    erb_e0: Tensor<f32>,
    erb_e1: Tensor<f32>,
    erb_e2: Tensor<f32>,
    erb_e3: Tensor<f32>,
    erb_h: Tensor<f32>,
    df_emb: Tensor<f32>,
    df_c0: Tensor<f32>,
    df_h: Tensor<f32>,
    df_c0_buf: Tensor<f32>,
    gamma_enc: Tensor<f32>,
    beta_enc: Tensor<f32>,
    gamma_df: Tensor<f32>,
    beta_df: Tensor<f32>,
}

impl V2OrtEngine {
    pub fn new(graphs: &ArchiveGraphs) -> Result<Self, String> {
        let enc = ort_session(&graphs.enc)?;
        let erb_dec = ort_session(&graphs.erb_dec)?;
        let df_dec = ort_session(&graphs.df_dec)?;
        if !(declares_film(&enc) && declares_film(&df_dec)) {
            return Err("the v2 graphs lack the FiLM inputs".to_owned());
        }
        let film = [1, 1, FILM_HIDDEN_DIM as i64];
        let mut engine = Self {
            feat_erb: ort_zeros(&[1, 1, 1, DEFAULT_NB_ERB as i64])?,
            feat_spec: ort_zeros(&[1, 2, 1, DEFAULT_NB_DF as i64])?,
            enc_h: ort_zeros(&[1, 1, GRU_HIDDEN])?,
            enc_erb_buf: ort_zeros(&[1, 1, 2, DEFAULT_NB_ERB as i64])?,
            enc_spec_buf: ort_zeros(&[1, 2, 2, DEFAULT_NB_DF as i64])?,
            erb_emb: ort_zeros(&[1, 1, EMB_DIM])?,
            erb_e0: ort_zeros(&[1, 64, 1, 32])?,
            erb_e1: ort_zeros(&[1, 64, 1, 16])?,
            erb_e2: ort_zeros(&[1, 64, 1, 8])?,
            erb_e3: ort_zeros(&[1, 64, 1, 8])?,
            erb_h: ort_zeros(&[2, 1, GRU_HIDDEN])?,
            df_emb: ort_zeros(&[1, 1, EMB_DIM])?,
            df_c0: ort_zeros(&[1, 64, 1, DEFAULT_NB_DF as i64])?,
            df_h: ort_zeros(&[2, 1, GRU_HIDDEN])?,
            df_c0_buf: ort_zeros(&[1, 64, 4, DEFAULT_NB_DF as i64])?,
            gamma_enc: ort_zeros(&film)?,
            beta_enc: ort_zeros(&film)?,
            gamma_df: ort_zeros(&film)?,
            beta_df: ort_zeros(&film)?,
            enc,
            erb_dec,
            df_dec,
        };
        engine.set_conditioning(&Conditioning::default())?;
        Ok(engine)
    }

    fn state_tensors(&mut self) -> [&mut Tensor<f32>; 6] {
        [
            &mut self.enc_h,
            &mut self.enc_erb_buf,
            &mut self.enc_spec_buf,
            &mut self.erb_h,
            &mut self.df_h,
            &mut self.df_c0_buf,
        ]
    }
}

impl Engine for V2OrtEngine {
    fn label(&self) -> String {
        "ort:CPU".to_owned()
    }

    fn run(&mut self, features: &ExtractedFeatures) -> Result<ModelFrameOutput, String> {
        if features
            .feat_erb
            .iter()
            .chain(features.feat_spec.iter().flatten())
            .any(|v| !v.is_finite())
        {
            return Err("feature frame contains a non-finite value".to_owned());
        }
        ort_fill(&mut self.feat_erb, &features.feat_erb)?;
        let mut spec = [0.0f32; 2 * DEFAULT_NB_DF];
        spec[..DEFAULT_NB_DF].copy_from_slice(&features.feat_spec[0]);
        spec[DEFAULT_NB_DF..].copy_from_slice(&features.feat_spec[1]);
        ort_fill(&mut self.feat_spec, &spec)?;

        let get = |o: &ort::session::SessionOutputs<'_>, name: &str| -> Result<Vec<f32>, String> {
            let v = o
                .get(name)
                .ok_or_else(|| format!("missing output `{name}`"))?;
            v.try_extract_tensor::<f32>()
                .map(|(_, d)| d.to_vec())
                .map_err(|e| e.to_string())
        };

        let enc_out = self
            .enc
            .run(vec![
                ("feat_erb", &self.feat_erb),
                ("feat_spec", &self.feat_spec),
                ("h_in", &self.enc_h),
                ("feat_erb_buf", &self.enc_erb_buf),
                ("feat_spec_buf", &self.enc_spec_buf),
                ("gamma", &self.gamma_enc),
                ("beta", &self.beta_enc),
            ])
            .map_err(|e| e.to_string())?;
        let lsnr = *get(&enc_out, "lsnr")?.first().ok_or("empty lsnr")?;
        let (e0, e1, e2, e3) = (
            get(&enc_out, "e0")?,
            get(&enc_out, "e1")?,
            get(&enc_out, "e2")?,
            get(&enc_out, "e3")?,
        );
        let (emb, c0) = (get(&enc_out, "emb")?, get(&enc_out, "c0")?);
        let enc_state = [
            get(&enc_out, "h_out")?,
            get(&enc_out, "feat_erb_buf_out")?,
            get(&enc_out, "feat_spec_buf_out")?,
        ];
        drop(enc_out);

        ort_fill(&mut self.erb_e0, &e0)?;
        ort_fill(&mut self.erb_e1, &e1)?;
        ort_fill(&mut self.erb_e2, &e2)?;
        ort_fill(&mut self.erb_e3, &e3)?;
        ort_fill(&mut self.erb_emb, &emb)?;
        let erb_out = self
            .erb_dec
            .run(vec![
                ("emb", &self.erb_emb),
                ("e3", &self.erb_e3),
                ("e2", &self.erb_e2),
                ("e1", &self.erb_e1),
                ("e0", &self.erb_e0),
                ("h_in", &self.erb_h),
            ])
            .map_err(|e| e.to_string())?;
        let mask = get(&erb_out, "m")?;
        let erb_h = get(&erb_out, "h_out")?;
        drop(erb_out);

        ort_fill(&mut self.df_emb, &emb)?;
        ort_fill(&mut self.df_c0, &c0)?;
        let df_out = self
            .df_dec
            .run(vec![
                ("emb", &self.df_emb),
                ("c0", &self.df_c0),
                ("h_in", &self.df_h),
                ("c0_buf", &self.df_c0_buf),
                ("gamma", &self.gamma_df),
                ("beta", &self.beta_df),
            ])
            .map_err(|e| e.to_string())?;
        let coefs = get(&df_out, "coefs")?;
        let df_h = get(&df_out, "h_out")?;
        let c0_buf = get(&df_out, "c0_buf_out")?;
        drop(df_out);

        let prediction = to_prediction(lsnr, &mask, &coefs)?;
        // Commit every state only after the three stages succeeded.
        let [a, b, c] = enc_state;
        for (tensor, values) in self
            .state_tensors()
            .into_iter()
            .zip([a, b, c, erb_h, df_h, c0_buf])
        {
            ort_fill(tensor, &values)?;
        }
        Ok(prediction)
    }

    fn reset(&mut self) -> Result<(), String> {
        for tensor in self.state_tensors() {
            let (_, buf) = tensor
                .try_extract_tensor_mut::<f32>()
                .map_err(|e| e.to_string())?;
            buf.fill(0.0);
        }
        Ok(())
    }

    fn set_conditioning(&mut self, conditioning: &Conditioning) -> Result<(), String> {
        let values = film_values(conditioning);
        if values
            .iter()
            .any(|v| v.len() != FILM_HIDDEN_DIM || v.iter().any(|x| !x.is_finite()))
        {
            return Err("FiLM vectors must hold 256 finite values".to_owned());
        }
        ort_fill(&mut self.gamma_enc, &values[0])?;
        ort_fill(&mut self.beta_enc, &values[1])?;
        ort_fill(&mut self.gamma_df, &values[2])?;
        ort_fill(&mut self.beta_df, &values[3])
    }

    fn supports_speaker(&self) -> bool {
        true
    }

    fn export_state(&self) -> Option<Vec<Vec<f32>>> {
        [
            &self.enc_h,
            &self.enc_erb_buf,
            &self.enc_spec_buf,
            &self.erb_h,
            &self.df_h,
            &self.df_c0_buf,
        ]
        .into_iter()
        .map(|t| ort_read(t).ok())
        .collect()
    }

    fn import_state(&mut self, state: &[Vec<f32>]) -> Result<(), String> {
        if state.len() != 6 {
            return Err("state layout differs".to_owned());
        }
        for (tensor, values) in self.state_tensors().into_iter().zip(state) {
            ort_fill(tensor, values)?;
        }
        Ok(())
    }
}

/// v2 graphs on one OpenVINO device ([`Dfn3Session`]). Its state cannot be exported, so a swap
/// to the ONNX Runtime standby starts from zeroed state (a few hops of re-convergence).
pub struct V2OpenVinoEngine {
    session: Dfn3Session,
    out: Dfn3Output,
}

impl V2OpenVinoEngine {
    pub fn new(graphs: &ArchiveGraphs, device: &str) -> Result<Self, String> {
        let graphs = Dfn3Graphs {
            enc: graphs.enc.clone(),
            erb_dec: graphs.erb_dec.clone(),
            df_dec: graphs.df_dec.clone(),
        };
        let session = Dfn3Session::load_graphs(&graphs, device).map_err(|e| e.to_string())?;
        if !session.supports_film() {
            return Err("the v2 graphs lack the FiLM inputs".to_owned());
        }
        Ok(Self {
            session,
            out: Dfn3Output::new(),
        })
    }
}

impl Engine for V2OpenVinoEngine {
    fn label(&self) -> String {
        format!("openvino:{}", self.session.device())
    }

    fn run(&mut self, features: &ExtractedFeatures) -> Result<ModelFrameOutput, String> {
        self.session
            .run_frame(&features.feat_erb, &features.feat_spec, &mut self.out)
            .map_err(|e| e.to_string())?;
        to_prediction(self.out.lsnr, &self.out.erb_mask, &self.out.df_coefs)
    }

    fn reset(&mut self) -> Result<(), String> {
        self.session.reset().map_err(|e| e.to_string())
    }

    fn set_conditioning(&mut self, conditioning: &Conditioning) -> Result<(), String> {
        let [ge, be, gd, bd] = film_values(conditioning);
        self.session
            .set_film(&ge, &be, &gd, &bd)
            .map_err(|e| e.to_string())
    }

    fn supports_speaker(&self) -> bool {
        true
    }
}
