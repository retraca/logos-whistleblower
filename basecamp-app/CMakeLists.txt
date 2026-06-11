cmake_minimum_required(VERSION 3.16)
project(whistleblower VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Qt6 REQUIRED COMPONENTS Quick Qml Network)

qt_add_qml_module(whistleblower_plugin
    URI com.logos.whistleblower
    VERSION 1.0
    SOURCES
        src/whistleblower_impl.h
        src/whistleblower_impl.cpp
        src/document_indexer_ffi.h
    QML_FILES
        qml/Main.qml
        qml/UploadPage.qml
        qml/IndexPage.qml
)

target_link_libraries(whistleblower_plugin PRIVATE
    Qt6::Quick
    Qt6::Qml
    Qt6::Network
    ${CMAKE_SOURCE_DIR}/../target/release/libdocument_indexer.a
)

install(TARGETS whistleblower_plugin DESTINATION .)
