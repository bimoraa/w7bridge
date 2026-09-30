/*! baseline과 양쪽 hash만 비교해서 전송 방향을 정해. */

pub(super) fn direction(
    uncertain: bool,
    baseline: Option<&Option<String>>,
    left: &Option<String>,
    right: &Option<String>,
) -> Option<bool> {

    if uncertain {

        None

    } else if baseline == Some(right) || baseline.is_none() && right.is_none() {

        Some(true)

    } else if baseline == Some(left) || baseline.is_none() && left.is_none() {

        Some(false)

    } else {

        None

    }

}
