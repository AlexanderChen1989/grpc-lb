cmake_minimum_required(VERSION 3.0.0)
project(nice VERSION 0.1.0)

include(CTest)
enable_testing()

add_executable(main)
target_sources(main 
	PRIVATE 
		main.cpp
)

target_link_libraries(main PRIVATE nice)

target_link_directories(main 
	PUBLIC 
		${CMAKE_CURRENT_SOURCE_DIR}/target/release/
)

set(CPACK_PROJECT_NAME ${PROJECT_NAME})
set(CPACK_PROJECT_VERSION ${PROJECT_VERSION})
include(CPack)
