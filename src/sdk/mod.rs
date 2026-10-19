pub mod sdk_dir;

const HEADERS: [&str; 4] = [
    "ThostFtdcTraderApi.h",
    "ThostFtdcMdApi.h",
    "ThostFtdcUserApiStruct.h",
    "ThostFtdcUserApiDataType.h",
];

const LIBS: [&str; 2] = ["thosttraderapi_se.so", "thostmduserapi_se.so"];
