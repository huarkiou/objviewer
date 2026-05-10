// Wavefront OBJ file parser
// Ported from WavefrontObjLoader.cs

use std::fs::File;
use std::io::{BufRead, BufReader};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Unsupported feature: {0}")]
    Unsupported(String),
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct ObjData {
    pub positions: Vec<[f32; 4]>,
    pub normals: Vec<[f32; 3]>,
    pub tex_coords: Vec<[f32; 2]>,
    pub faces: Vec<Face>,
    pub has_normals: bool,
    pub num_faces: usize,
    pub num_indices: usize,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Face {
    pub v_idx: Vec<i32>,
    pub n_idx: Vec<i32>,
    pub tex_idx: Vec<i32>,
}

impl ObjData {
    pub fn load(path: &str) -> Result<Self, ParseError> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        let mut positions: Vec<[f32; 4]> = Vec::new();
        let mut normals: Vec<[f32; 3]> = Vec::new();
        let mut tex_coords: Vec<[f32; 2]> = Vec::new();
        let mut faces: Vec<Face> = Vec::new();
        let mut has_normals = false;
        let mut num_indices: usize = 0;

        let mut line = String::new();
        while reader.read_line(&mut line)? > 0 {
            // Handle line continuation with trailing '\'
            loop {
                if line.ends_with("\\\r\n") {
                    // Remove the continuation marker (backslash + CRLF)
                    line.truncate(line.len() - 3);
                } else if line.ends_with("\\\n") {
                    // Remove the continuation marker (backslash + LF)
                    line.truncate(line.len() - 2);
                } else {
                    break;
                }
                // Read and append the next physical line
                let mut next = String::new();
                if reader.read_line(&mut next)? == 0 {
                    break; // EOF, stop merging
                }
                line.push_str(&next);
            }

            // Strip trailing newline and whitespace for processing
            let trimmed = line.trim();

            // Skip empty lines and full-line comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                line.clear();
                continue;
            }

            // Split into first token and remaining content (max 2 parts)
            let mut parts = trimmed.splitn(2, char::is_whitespace);
            let first_token = parts.next().unwrap_or("");
            let rest = parts.next().unwrap_or("");

            match first_token {
                // Comments (already handled above, but defensive)
                "#" => {}

                // Material references — silently ignored
                "mtllib" | "usemtl" => {}

                // Unsupported but non-fatal — log warning
                "o" | "g" | "s" | "shadow_obj" | "trace_obj" => {
                    log::warn!("unsupported wavefront token: {}", first_token);
                }

                // Vertex position: 3 or 4 floats (w defaults to 0.0)
                "v" => {
                    let pos = Self::parse_vector4(rest)?;
                    positions.push(pos);
                }

                // Vertex normal: exactly 3 floats
                "vn" => {
                    let n = Self::parse_vector3(rest)?;
                    normals.push(n);
                }

                // Vertex texture coordinate: exactly 2 floats
                "vt" => {
                    let tc = Self::parse_vector2(rest)?;
                    tex_coords.push(tc);
                }

                // Face: supports v, v/vt, v//vn, v/vt/vn formats
                "f" => {
                    let values: Vec<&str> =
                        rest.split_whitespace().filter(|s| !s.is_empty()).collect();
                    let num_points = values.len();

                    let mut v_idx = vec![-1i32; num_points];
                    let mut n_idx = vec![-1i32; num_points];
                    let mut tex_idx = vec![-1i32; num_points];

                    for i in 0..num_points {
                        let indexes: Vec<&str> = values[i].split('/').collect();

                        // Parse position index (always present, first component)
                        let i_pos_raw: i32 = indexes[0].parse().map_err(|_| {
                            ParseError::Parse(format!("Invalid vertex index: {}", indexes[0]))
                        })?;
                        // Convert from 1-based to 0-based
                        let mut i_pos = i_pos_raw - 1;
                        // Handle negative indices (wrap from end of list)
                        if i_pos < 0 {
                            i_pos += positions.len() as i32 + 1;
                        }
                        v_idx[i] = i_pos;
                        num_indices += 1;

                        // Handle optional texture coordinate and normal indices
                        if indexes.len() > 1 {
                            let tex_index = indexes[1];
                            if !tex_index.is_empty() {
                                let i_tex_raw: i32 = tex_index.parse().map_err(|_| {
                                    ParseError::Parse(format!(
                                        "Invalid tex coord index: {}",
                                        tex_index
                                    ))
                                })?;
                                let mut i_tex = i_tex_raw - 1;
                                if i_tex < 0 {
                                    i_tex += tex_coords.len() as i32 + 1;
                                }
                                tex_idx[i] = i_tex;
                            }

                            if indexes.len() > 2 {
                                has_normals = true;
                                let i_norm_raw: i32 = indexes[2].parse().map_err(|_| {
                                    ParseError::Parse(format!(
                                        "Invalid normal index: {}",
                                        indexes[2]
                                    ))
                                })?;
                                let mut i_norm = i_norm_raw - 1;
                                if i_norm < 0 {
                                    i_norm += normals.len() as i32 + 1;
                                }
                                n_idx[i] = i_norm;
                            }
                        }
                    }

                    faces.push(Face {
                        v_idx,
                        n_idx,
                        tex_idx,
                    });
                }

                // Fatal error: curve/surface tokens not supported
                "cstype" | "deg" | "step" | "bmat" | "surf" | "parm" | "trim" | "hole" | "scrv"
                | "sp" | "end" | "con" | "vp" | "bevel" | "c_interp" | "d_interp" | "lod"
                | "ctech" | "stech" | "mg" => {
                    return Err(ParseError::Unsupported(format!(
                        "fatal error, token not supported: {}",
                        first_token
                    )));
                }

                // Unknown tokens: silently ignore (same as C# fall-through behavior)
                _ => {}
            }

            line.clear();
        }

        let num_faces = faces.len();

        Ok(ObjData {
            positions,
            normals,
            tex_coords,
            faces,
            has_normals,
            num_faces,
            num_indices,
        })
    }

    /// Parse 3 or 4 floats from whitespace-separated string.
    /// Returns [x, y, z, w] where w defaults to 0.0 if only 3 values present.
    fn parse_vector4(s: &str) -> Result<[f32; 4], ParseError> {
        let values: Vec<&str> = s.split_whitespace().collect();
        match values.len() {
            3 => Ok([
                values[0]
                    .parse()
                    .map_err(|_| ParseError::Parse(format!("Invalid float: {}", values[0])))?,
                values[1]
                    .parse()
                    .map_err(|_| ParseError::Parse(format!("Invalid float: {}", values[1])))?,
                values[2]
                    .parse()
                    .map_err(|_| ParseError::Parse(format!("Invalid float: {}", values[2])))?,
                0.0,
            ]),
            4 => Ok([
                values[0]
                    .parse()
                    .map_err(|_| ParseError::Parse(format!("Invalid float: {}", values[0])))?,
                values[1]
                    .parse()
                    .map_err(|_| ParseError::Parse(format!("Invalid float: {}", values[1])))?,
                values[2]
                    .parse()
                    .map_err(|_| ParseError::Parse(format!("Invalid float: {}", values[2])))?,
                values[3]
                    .parse()
                    .map_err(|_| ParseError::Parse(format!("Invalid float: {}", values[3])))?,
            ]),
            _ => Err(ParseError::Parse(format!(
                "Expected 3 or 4 values for vertex position, got {}: {}",
                values.len(),
                s
            ))),
        }
    }

    /// Parse exactly 3 floats from whitespace-separated string.
    fn parse_vector3(s: &str) -> Result<[f32; 3], ParseError> {
        let values: Vec<&str> = s.split_whitespace().collect();
        if values.len() != 3 {
            return Err(ParseError::Parse(format!(
                "Expected 3 values for vertex normal, got {}: {}",
                values.len(),
                s
            )));
        }
        Ok([
            values[0]
                .parse()
                .map_err(|_| ParseError::Parse(format!("Invalid float: {}", values[0])))?,
            values[1]
                .parse()
                .map_err(|_| ParseError::Parse(format!("Invalid float: {}", values[1])))?,
            values[2]
                .parse()
                .map_err(|_| ParseError::Parse(format!("Invalid float: {}", values[2])))?,
        ])
    }

    /// Parse exactly 2 floats from whitespace-separated string.
    fn parse_vector2(s: &str) -> Result<[f32; 2], ParseError> {
        let values: Vec<&str> = s.split_whitespace().collect();
        if values.len() != 2 {
            return Err(ParseError::Parse(format!(
                "Expected 2 values for tex coord, got {}: {}",
                values.len(),
                s
            )));
        }
        Ok([
            values[0]
                .parse()
                .map_err(|_| ParseError::Parse(format!("Invalid float: {}", values[0])))?,
            values[1]
                .parse()
                .map_err(|_| ParseError::Parse(format!("Invalid float: {}", values[1])))?,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Helper: write a string to a temp file and return the path.
    fn write_temp(filename: &str, content: &str) -> String {
        let dir = std::env::temp_dir();
        let path = dir.join(filename);
        let mut file = File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path.to_str().unwrap().to_string()
    }

    // ── Test 1: Basic cube parse (from test_fixtures/cube.obj) ──

    #[test]
    fn test_parse_cube() {
        // Use the real fixture file
        let result = ObjData::load("test_fixtures/cube.obj");
        assert!(
            result.is_ok(),
            "Failed to parse cube.obj: {:?}",
            result.err()
        );

        let obj = result.unwrap();
        assert_eq!(obj.positions.len(), 8, "Cube should have 8 vertices");
        assert_eq!(obj.normals.len(), 6, "Cube should have 6 normals");
        assert_eq!(obj.tex_coords.len(), 0, "Cube has no tex coords");
        assert_eq!(obj.faces.len(), 12, "Cube should have 12 triangular faces");
        assert_eq!(obj.num_faces, 12);
        assert_eq!(
            obj.num_indices,
            12 * 3,
            "12 faces × 3 vertices = 36 indices"
        );
        assert!(obj.has_normals, "Cube face definitions include normals");

        // Verify first vertex
        assert_eq!(obj.positions[0], [0.0, 0.0, 0.0, 0.0]);

        // Verify first face uses 0-based indices
        let f0 = &obj.faces[0];
        assert_eq!(f0.v_idx, vec![0, 1, 2]); // 1→0, 2→1, 3→2
        assert_eq!(f0.n_idx, vec![0, 0, 0]); // all reference normal 1 → index 0
        assert_eq!(f0.tex_idx, vec![-1, -1, -1]); // no tex coords
    }

    // ── Test 2: Basic triangle parse (from test_fixtures/triangle.obj) ──

    #[test]
    fn test_parse_triangle() {
        let result = ObjData::load("test_fixtures/triangle.obj");
        assert!(
            result.is_ok(),
            "Failed to parse triangle.obj: {:?}",
            result.err()
        );

        let obj = result.unwrap();
        assert_eq!(obj.positions.len(), 3);
        assert_eq!(obj.normals.len(), 1);
        assert_eq!(obj.faces.len(), 1);
        assert_eq!(obj.num_faces, 1);
        assert_eq!(obj.num_indices, 3);
        assert!(obj.has_normals);

        let face = &obj.faces[0];
        assert_eq!(face.v_idx, vec![0, 1, 2]);
        assert_eq!(face.n_idx, vec![0, 0, 0]);
    }

    // ── Test 3: Face without normals (v and v/vt format) ──

    #[test]
    fn test_face_without_normals() {
        let content = "\
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 0.5 1.0 0.0
vt 0.0 0.0
vt 1.0 0.0
vt 0.5 1.0
f 1/1 2/2 3/3
";
        let path = write_temp("test_no_normals.obj", content);

        let obj = ObjData::load(&path).unwrap();
        assert_eq!(obj.faces.len(), 1);
        assert!(!obj.has_normals);

        let face = &obj.faces[0];
        assert_eq!(face.v_idx, vec![0, 1, 2]);
        assert_eq!(face.tex_idx, vec![0, 1, 2]);
        assert_eq!(face.n_idx, vec![-1, -1, -1]); // no normals → -1
    }

    // ── Test 4: Face with normals (v//vn format) ──

    #[test]
    fn test_face_with_normals() {
        let content = "\
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 0.5 1.0 0.0
vn 0.0 0.0 1.0
vn 0.0 0.0 1.0
vn 0.0 0.0 1.0
f 1//1 2//2 3//3
";
        let path = write_temp("test_with_normals.obj", content);

        let obj = ObjData::load(&path).unwrap();
        assert!(obj.has_normals);

        let face = &obj.faces[0];
        assert_eq!(face.v_idx, vec![0, 1, 2]);
        assert_eq!(face.n_idx, vec![0, 1, 2]);
        assert_eq!(face.tex_idx, vec![-1, -1, -1]); // no tex coords → -1
    }

    // ── Test 5: Negative index handling ──

    #[test]
    fn test_negative_indices() {
        let content = "\
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 0.5 1.0 0.0
vn 0.0 0.0 1.0
vn 0.0 0.0 -1.0
# -1 refers to last vertex (3 → index 2), -2 refers to second-to-last (2 → index 1)
# -1 for normal refers to last normal (2 → index 1)
f -1//-1 -2//-2 -3//-1
";
        let path = write_temp("test_neg_indices.obj", content);

        let obj = ObjData::load(&path).unwrap();
        assert_eq!(obj.positions.len(), 3);
        assert_eq!(obj.normals.len(), 2);

        let face = &obj.faces[0];
        // -1 (1-based) → 0-based: -1-1=-2; -2 += 3+1 = 2 → index 2 (last)
        // -2 (1-based) → 0-based: -2-1=-3; -3 += 3+1 = 1 → index 1
        // -3 (1-based) → 0-based: -3-1=-4; -4 += 3+1 = 0 → index 0
        assert_eq!(face.v_idx, vec![2, 1, 0]);
        // -1 → last normal: -1-1=-2; -2 += 2+1 = 1 → index 1
        // -2 → second-to-last normal: -2-1=-3; -3 += 2+1 = 0 → index 0
        assert_eq!(face.n_idx, vec![1, 0, 1]);
    }

    // ── Test 6: Comment skipping ──

    #[test]
    fn test_comment_skipping() {
        let content = "\
# This is a full-line comment
v 0.0 0.0 0.0
# Another comment
v 1.0 0.0 0.0
v 0.5 1.0 0.0
#comment without space
f 1 2 3
";
        let path = write_temp("test_comments.obj", content);

        let obj = ObjData::load(&path).unwrap();
        // Only 3 vertices should be parsed, comments ignored
        assert_eq!(obj.positions.len(), 3);
        assert_eq!(obj.faces.len(), 1);
    }

    // ── Test 7: Line continuation ──

    #[test]
    fn test_line_continuation() {
        let content = "\
v 0.0 0.0 \\\r
0.0
v 1.0 0.0 0.0
v 0.5 1.0 0.0
f 1 2 3
";
        let path = write_temp("test_continuation.obj", content);

        let obj = ObjData::load(&path).unwrap();
        assert_eq!(obj.positions.len(), 3);
        // First vertex should have z=0.0 (from the continued line)
        assert_eq!(obj.positions[0], [0.0, 0.0, 0.0, 0.0]);
    }

    // ── Test 8: mtllib/usemtl are silently ignored ──

    #[test]
    fn test_material_ignored() {
        let content = "\
mtllib test.mtl
usemtl red
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 0.5 1.0 0.0
f 1 2 3
";
        let path = write_temp("test_material.obj", content);

        let obj = ObjData::load(&path).unwrap();
        assert_eq!(obj.positions.len(), 3);
        assert_eq!(obj.faces.len(), 1);
    }

    // ── Test 9: Unsupported curve token is fatal ──

    #[test]
    fn test_unsupported_curve_token() {
        let content = "\
v 0.0 0.0 0.0
v 1.0 0.0 0.0
cstype bezier
";
        let path = write_temp("test_curve.obj", content);

        let result = ObjData::load(&path);
        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::Unsupported(msg) => {
                assert!(msg.contains("cstype"));
            }
            _ => panic!("Expected Unsupported error"),
        }
    }

    // ── Test 10: 4-component vertex (w coordinate) ──

    #[test]
    fn test_vertex_with_w() {
        let content = "\
v 1.0 2.0 3.0 4.0
";
        let path = write_temp("test_vertex_w.obj", content);

        let obj = ObjData::load(&path).unwrap();
        assert_eq!(obj.positions[0], [1.0, 2.0, 3.0, 4.0]);
    }
}
