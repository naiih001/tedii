pub fn fuzzy_score(query: &str, target: &str) -> Option<(i64, Vec<usize>)> {
    let query = query.to_lowercase();
    let target = target.to_lowercase();
    let qchars: Vec<char> = query.chars().collect();
    let tchars: Vec<char> = target.chars().collect();

    if qchars.is_empty() {
        return None;
    }

    let mut score: i64 = 0;
    let mut indices = Vec::new();
    let mut qi = 0;

    for (ti, tc) in tchars.iter().enumerate() {
        if qi < qchars.len() && *tc == qchars[qi] {
            indices.push(ti);
            if qi > 0 && indices[qi - 1] == ti - 1 {
                score += 15;
            }
            if ti == 0 || matches!(tchars[ti - 1], '/' | '-' | '_' | '.' | ' ') {
                score += 20;
            }
            score += 10;
            qi += 1;
        }
    }

    if qi == qchars.len() {
        if indices.len() > 1 {
            let gap = *indices.last().unwrap() as i64
                - *indices.first().unwrap() as i64
                - indices.len() as i64
                + 1;
            score -= 3 * gap;
        }
        Some((score, indices))
    } else {
        None
    }
}
