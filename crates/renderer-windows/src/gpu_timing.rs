//! Optional GPU timestamps. Four reusable query sets; never wait or flush the GPU.
use media_core::diagnostics::StageMetrics;
use std::{collections::VecDeque, ptr, time::Duration};
use windows::{
    core::{Interface, Result},
    Win32::{
        Foundation::S_OK,
        Graphics::Direct3D11::{
            ID3D11Device, ID3D11DeviceContext, ID3D11Query, D3D11_ASYNC_GETDATA_DONOTFLUSH,
            D3D11_QUERY_DATA_TIMESTAMP_DISJOINT, D3D11_QUERY_DESC, D3D11_QUERY_TIMESTAMP,
            D3D11_QUERY_TIMESTAMP_DISJOINT,
        },
    },
};
struct Queries {
    disjoint: ID3D11Query,
    start: ID3D11Query,
    end: ID3D11Query,
}
pub struct GpuTiming {
    queries: Vec<Queries>,
    pending: VecDeque<usize>,
    active: Option<usize>,
    skipped: u64,
    invalid: u64,
}
impl GpuTiming {
    pub fn new(device: &ID3D11Device) -> Result<Self> {
        let mut queries = Vec::new();
        for _ in 0..4 {
            queries.push(Queries {
                disjoint: query(device, true)?,
                start: query(device, false)?,
                end: query(device, false)?,
            });
        }
        Ok(Self {
            queries,
            pending: VecDeque::new(),
            active: None,
            skipped: 0,
            invalid: 0,
        })
    }
    pub fn begin(&mut self, context: &ID3D11DeviceContext, metrics: &mut StageMetrics) {
        self.collect(context, metrics);
        let Some(index) = (0..4).find(|index| !self.pending.contains(index)) else {
            self.skipped += 1;
            return;
        };
        self.active = Some(index);
        // SAFETY: queries belong to this context's device; pairs end before reuse.
        unsafe {
            context.Begin(&self.queries[index].disjoint);
            context.End(&self.queries[index].start);
        }
    }
    pub fn end(&mut self, context: &ID3D11DeviceContext) {
        if let Some(index) = self.active.take() {
            // SAFETY: closes the query pair opened by begin on the same owner thread.
            unsafe {
                context.End(&self.queries[index].end);
                context.End(&self.queries[index].disjoint);
            }
            self.pending.push_back(index);
        }
    }
    pub fn collect(&mut self, context: &ID3D11DeviceContext, metrics: &mut StageMetrics) {
        while let Some(&index) = self.pending.front() {
            let q = &self.queries[index];
            let mut data = D3D11_QUERY_DATA_TIMESTAMP_DISJOINT::default();
            let (mut start, mut end) = (0_u64, 0_u64);
            if !read(context, &q.disjoint, &mut data)
                || !read(context, &q.start, &mut start)
                || !read(context, &q.end, &mut end)
            {
                break;
            }
            self.pending.pop_front();
            if data.Disjoint.as_bool() || data.Frequency == 0 || end < start {
                self.invalid += 1;
                continue;
            }
            let ns = u128::from(end - start) * 1_000_000_000 / u128::from(data.Frequency);
            metrics.record(
                "render_gpu_upload_draw",
                Duration::from_nanos(u64::try_from(ns).unwrap_or(u64::MAX)),
            );
        }
    }
    pub fn report(&self) {
        tracing::info!(target: "telemetry",
            gpu_query_pending = self.pending.len(),
            gpu_query_skipped = self.skipped,
            gpu_query_invalid = self.invalid,
            "GPU timing health interval"
        );
    }
}
fn query(device: &ID3D11Device, disjoint: bool) -> Result<ID3D11Query> {
    let mut output = None;
    let desc = D3D11_QUERY_DESC {
        Query: if disjoint {
            D3D11_QUERY_TIMESTAMP_DISJOINT
        } else {
            D3D11_QUERY_TIMESTAMP
        },
        MiscFlags: 0,
    };
    // SAFETY: valid descriptor and initialized owned COM out pointer.
    unsafe {
        device.CreateQuery(ptr::from_ref(&desc), Some(ptr::addr_of_mut!(output)))?;
    }
    Ok(output.unwrap())
}
fn read<T>(context: &ID3D11DeviceContext, query: &ID3D11Query, output: &mut T) -> bool {
    // SAFETY: callers use exactly the data type defined by the query; queries are
    // alive and output is a valid writable allocation. Raw HRESULT preserves
    // S_FALSE (not ready), which the Result wrapper would treat as success.
    unsafe {
        (Interface::vtable(context).GetData)(
            Interface::as_raw(context),
            Interface::as_raw(query),
            ptr::from_mut(output).cast(),
            u32::try_from(size_of::<T>()).unwrap(),
            D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32,
        ) == S_OK
    }
}
