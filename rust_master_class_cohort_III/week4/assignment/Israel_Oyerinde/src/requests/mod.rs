mod create_book;
mod queries;
mod replace_book;
mod update_book;
mod validate;

pub use create_book::CreateBookRequest;
pub use queries::{ListBooksQuery, SearchBooksQuery};
pub use replace_book::ReplaceBookRequest;
pub use update_book::UpdateBookRequest;
pub use validate::Validate;

pub(crate) use validate::{validate_author, validate_genre, validate_title};
