use crate::domain::operations::ItemStatus;
use windows::{
    Win32::UI::Shell::{
        COPYENGINE_S_ALREADY_DONE, COPYENGINE_S_DONT_PROCESS_CHILDREN, COPYENGINE_S_USER_IGNORED,
    },
    core::HRESULT,
};

pub(super) fn item_status(
    result: HRESULT,
    has_destination: bool,
    relocation_confirmed: bool,
) -> ItemStatus {
    if result.is_err() {
        ItemStatus::Failed
    } else if result == COPYENGINE_S_USER_IGNORED || result == COPYENGINE_S_ALREADY_DONE {
        ItemStatus::Skipped
    } else if has_destination
        && (result.0 == 0 || (result == COPYENGINE_S_DONT_PROCESS_CHILDREN && relocation_confirmed))
    {
        ItemStatus::Changed
    } else {
        ItemStatus::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Shell::{COPYENGINE_S_MERGE, COPYENGINE_S_PENDING};

    #[test]
    fn relocation_special_success_needs_actual_identity_but_skips_and_unknown_codes_never_succeed()
    {
        assert_eq!(
            item_status(COPYENGINE_S_DONT_PROCESS_CHILDREN, true, true),
            ItemStatus::Changed
        );
        assert_eq!(
            item_status(COPYENGINE_S_DONT_PROCESS_CHILDREN, true, false),
            ItemStatus::Unknown
        );
        assert_eq!(
            item_status(COPYENGINE_S_DONT_PROCESS_CHILDREN, false, true),
            ItemStatus::Unknown
        );
        for code in [COPYENGINE_S_USER_IGNORED, COPYENGINE_S_ALREADY_DONE] {
            assert_eq!(item_status(code, true, true), ItemStatus::Skipped);
        }
        for code in [COPYENGINE_S_PENDING, COPYENGINE_S_MERGE, HRESULT(12345)] {
            assert_eq!(item_status(code, true, true), ItemStatus::Unknown);
        }
    }
}
