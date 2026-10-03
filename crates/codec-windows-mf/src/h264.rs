#[derive(Debug, Default)]
pub(crate) struct ParameterSetCache {
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
}

impl ParameterSetCache {
    pub(crate) fn normalize(&mut self, access_unit: &[u8]) -> (Vec<u8>, bool) {
        let units = annex_b_units(access_unit);
        let mut keyframe = false;
        let mut sequence_present = false;
        let mut picture_present = false;
        for unit in &units {
            let Some(kind) = unit.first().map(|byte| byte & 0x1f) else {
                continue;
            };
            match kind {
                5 => keyframe = true,
                7 => {
                    self.sps = Some((*unit).to_vec());
                    sequence_present = true;
                }
                8 => {
                    self.pps = Some((*unit).to_vec());
                    picture_present = true;
                }
                _ => {}
            }
        }
        if !keyframe || (sequence_present && picture_present) {
            return (access_unit.to_vec(), keyframe);
        }
        let mut normalized = Vec::with_capacity(
            access_unit.len()
                + self.sps.as_ref().map_or(0, |value| value.len() + 4)
                + self.pps.as_ref().map_or(0, |value| value.len() + 4),
        );
        if !sequence_present {
            append_unit(&mut normalized, self.sps.as_deref());
        }
        if !picture_present {
            append_unit(&mut normalized, self.pps.as_deref());
        }
        normalized.extend_from_slice(access_unit);
        (normalized, keyframe)
    }
}

fn append_unit(destination: &mut Vec<u8>, unit: Option<&[u8]>) {
    if let Some(unit) = unit {
        destination.extend_from_slice(&[0, 0, 0, 1]);
        destination.extend_from_slice(unit);
    }
}

fn annex_b_units(bytes: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let length = if index + 4 <= bytes.len() && bytes[index..index + 4] == [0, 0, 0, 1] {
            4
        } else if bytes[index..index + 3] == [0, 0, 1] {
            3
        } else {
            index += 1;
            continue;
        };
        starts.push((index, index + length));
        index += length;
    }
    starts
        .iter()
        .enumerate()
        .filter_map(|(position, (_, start))| {
            let end = starts
                .get(position + 1)
                .map_or(bytes.len(), |(code_start, _)| *code_start);
            (*start < end).then_some(&bytes[*start..end])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_and_prepends_parameter_sets_to_later_idr() {
        let mut cache = ParameterSetCache::default();
        let first = [0, 0, 0, 1, 0x67, 1, 2, 0, 0, 1, 0x68, 3, 0, 0, 1, 0x65, 4];
        let (normalized, keyframe) = cache.normalize(&first);
        assert!(keyframe);
        assert_eq!(normalized, first);

        let later = [0, 0, 1, 0x65, 9];
        let (normalized, keyframe) = cache.normalize(&later);
        assert!(keyframe);
        assert!(normalized.starts_with(&[0, 0, 0, 1, 0x67, 1, 2]));
        assert!(normalized.ends_with(&later));
    }

    #[test]
    fn non_idr_is_not_modified() {
        let mut cache = ParameterSetCache::default();
        let access_unit = [0, 0, 1, 0x41, 7];
        assert_eq!(cache.normalize(&access_unit), (access_unit.to_vec(), false));
    }
}
