use std::fmt::Display;

use strum::EnumIter;

#[derive(Debug, EnumIter)]
pub enum FakerType {
    FirstName,
    LastName,
    Name,
    CompanyName,
    Email,
    PhoneNumber,
    Address,
    Md5,
}

impl Display for FakerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FakerType::FirstName => write!(f, "fake_firstname_transformation"),
            FakerType::LastName => write!(f, "fake_lastname_transformation"),
            FakerType::Name => write!(f, "fake_name_transformation"),
            FakerType::CompanyName => write!(f, "fake_companyname_transformation"),
            FakerType::Email => write!(f, "fake_email_transformation"),
            FakerType::PhoneNumber => write!(f, "fake_phone_transformation"),
            FakerType::Address => write!(f, "fake_address_transformation"),
            FakerType::Md5 => write!(f, "fake_md5_transformation"),
        }
    }
}
